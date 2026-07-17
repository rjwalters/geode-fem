"""Regression test for #199: ``refs_pdf.py`` must import cleanly without a
top-level ``anvil`` package on ``sys.path`` (i.e., in the consumer-install
layout, where the framework lands at ``.anvil/`` with no top-level
``anvil/`` package on ``sys.path``).

Background
----------

Issue #199 surfaced via the studio canary: every consumer install of
``anvil:memo`` would dangle ``from anvil.lib.render import RenderError``
on module load of ``anvil/skills/memo/lib/refs_pdf.py``, blocking the
documented ``memo-review`` step 5 procedure on every install. The root
cause was a namespace mismatch — ``install-anvil.sh`` *does* copy
``anvil/lib/render.py`` into the consumer's ``.anvil/lib/render.py``,
but there is no top-level ``anvil/`` package on the consumer's
``sys.path``, so any runtime import of ``anvil.lib.*`` from inside a
``.anvil/skills/<skill>/lib/`` module dangles.

The fix (option (b) per the curated issue body) is to drop the
``anvil.lib.render`` import and define ``RenderError`` skill-locally as a
``RuntimeError`` subclass. This file pins that contract: ``refs_pdf.py``
must import successfully with NO ``anvil/`` package available on
``sys.path``.

Test design
-----------

The test copies ``refs_pdf.py`` into an isolated ``tmp_path`` directory
and invokes ``python -c "import sys; sys.path = [<isolated>]; import
refs_pdf"`` in a subprocess. The subprocess invocation is load-bearing:
it isolates ``sys.path`` from the test-runner's environment, so the
test cannot accidentally pass just because the repo-root ``anvil/``
package happens to be importable at test time (it always is, when the
suite is run from the repo root).

The subprocess REPLACES ``sys.path`` rather than just prepending the
isolated dir, to guarantee no fallback to the repo-root ``anvil/``
package. Stdlib is still importable because we keep the standard
``PYTHONPATH``-derived entries via ``sys.executable``'s built-in path
config — only user-site / cwd-style entries that might surface
``anvil/`` are stripped.

Per the #58 packaging convention, this file's filename
(``test_memo_refs_pdf_standalone_import.py``) is unique across the
``anvil/skills/*/tests/`` tree so the cross-skill ``pytest`` discovery
does not collide on basename.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


_HERE = Path(__file__).resolve().parent
_REFS_PDF = _HERE.parent / "lib" / "refs_pdf.py"


class TestRefsPdfStandaloneImport(unittest.TestCase):
    """``refs_pdf.py`` MUST import without a top-level ``anvil/`` package.

    Pins the consumer-install contract from issue #199: the file is
    copied into ``.anvil/skills/memo/lib/refs_pdf.py`` on a consumer
    install, and ``.anvil/`` is NOT a top-level Python package on
    ``sys.path`` — so any ``anvil.*`` runtime import inside this file
    would dangle on every install.
    """

    def test_refs_pdf_file_present(self) -> None:
        """Defensive: the module file must exist at the documented path.

        If this fails, the rest of this file's assertions are moot — and
        a prior refactor has moved the module without updating this
        regression guard.
        """
        self.assertTrue(
            _REFS_PDF.exists(),
            f"refs_pdf.py not found at {_REFS_PDF}; module-layout drift",
        )

    def test_refs_pdf_imports_without_anvil_namespace(self) -> None:
        """``import refs_pdf`` must succeed with no ``anvil/`` on sys.path.

        Simulates the consumer-install layout (``.anvil/skills/memo/lib/
        refs_pdf.py`` with no top-level ``anvil/`` package). On success
        the subprocess prints the resolved ``RenderError`` class repr to
        stdout so this test also doubles as a positive assertion that the
        skill-local mirror is in place (issue #199 acceptance criterion).
        """
        import tempfile

        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp_path = Path(raw_tmp)
            isolated = tmp_path / "refs_pdf.py"
            isolated.write_bytes(_REFS_PDF.read_bytes())

            # Build a hermetic sys.path containing ONLY the isolated dir
            # plus the standard-library entries Python configures
            # automatically. We strip anything that could surface a
            # top-level ``anvil/`` package (cwd, user-site, repo-root in
            # PYTHONPATH) — and we run the subprocess with PYTHONPATH
            # unset and cwd set to a directory other than the repo root
            # so the test cannot accidentally pass via the repo's own
            # ``anvil/`` package being importable.
            program = textwrap.dedent(
                """
                import sys
                # Replace sys.path with ONLY the isolated dir. We
                # additionally restore the stdlib + site-packages entries
                # that the Python launcher pre-loaded under the original
                # sys.path[0] equivalents, EXCEPT any "" / "." / repo-root
                # entries that could leak the framework's own ``anvil/``
                # package. site-packages stays so pydantic etc. remain
                # importable (refs_pdf itself does not need them, but
                # this keeps the smoke test future-proof).
                isolated = sys.argv[1]
                sys.path = [
                    p for p in sys.path
                    if p and p not in ("", ".")
                ]
                sys.path.insert(0, isolated)
                # Hard guarantee: ``anvil`` must NOT be importable in
                # this configuration. If it is, the test environment
                # has leaked the repo-root package and the assertion
                # below would be a false positive.
                try:
                    import anvil  # noqa: F401
                    print("LEAK: top-level anvil/ is on sys.path")
                    sys.exit(2)
                except ModuleNotFoundError:
                    pass
                import refs_pdf
                # Smoke: the skill-local mirror must exist and be a
                # ``RuntimeError`` subclass per the issue #199 fix.
                assert issubclass(refs_pdf.RenderError, RuntimeError), (
                    "refs_pdf.RenderError is not a RuntimeError subclass: "
                    + repr(refs_pdf.RenderError.__mro__)
                )
                assert "RenderError" in refs_pdf.__all__, (
                    "refs_pdf.__all__ does not export RenderError: "
                    + repr(refs_pdf.__all__)
                )
                print("OK", refs_pdf.RenderError.__qualname__)
                """
            ).strip()

            # Run in a hermetic cwd (tmp dir parent), with PYTHONPATH
            # cleared so the repo-root ``anvil/`` package cannot be
            # imported via the environment.
            import os

            env = {
                k: v
                for k, v in os.environ.items()
                if k != "PYTHONPATH"
            }
            result = subprocess.run(
                [sys.executable, "-c", program, str(tmp_path)],
                capture_output=True,
                text=True,
                cwd=str(tmp_path),
                env=env,
            )
            self.assertEqual(
                result.returncode,
                0,
                msg=(
                    "Standalone import of refs_pdf.py failed (issue #199 "
                    "regression). stderr:\n"
                    + (result.stderr or "<empty>")
                    + "\nstdout:\n"
                    + (result.stdout or "<empty>")
                ),
            )
            self.assertIn("OK RenderError", result.stdout)

    def test_refs_pdf_has_no_anvil_runtime_imports(self) -> None:
        """Belt-and-braces: ``refs_pdf.py`` must contain zero
        ``anvil.*`` runtime imports.

        A static-substring guard (the runtime test above covers the
        subprocess path, but this also catches a regression in a docstring
        example or a half-applied refactor before the subprocess test
        executes).
        """
        body = _REFS_PDF.read_text(encoding="utf-8")
        # Strip the module docstring so the assertion targets only
        # executable lines. The docstring is allowed to reference
        # ``anvil.lib.render`` for context; what must NOT appear is an
        # actual runtime import.
        # Simple approach: split on the first ``"""`` terminator after
        # the leading docstring.
        # The module starts with ``"""...`` followed by a closing
        # ``"""`` on its own line — find that and slice past it.
        first_triple = body.find('"""')
        self.assertEqual(
            first_triple, 0, "refs_pdf.py does not start with a docstring"
        )
        second_triple = body.find('"""', first_triple + 3)
        self.assertGreater(
            second_triple,
            first_triple,
            "refs_pdf.py docstring is unterminated",
        )
        executable = body[second_triple + 3 :]

        self.assertNotIn(
            "from anvil.",
            executable,
            "refs_pdf.py contains a runtime ``from anvil.*`` import "
            "(issue #199 regression): consumer installs land at "
            "``.anvil/`` with no top-level ``anvil/`` package on sys.path",
        )
        self.assertNotIn(
            "import anvil",
            executable,
            "refs_pdf.py contains a runtime ``import anvil*`` statement "
            "(issue #199 regression): consumer installs land at "
            "``.anvil/`` with no top-level ``anvil/`` package on sys.path",
        )


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
