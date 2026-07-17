"""Tests for ``anvil/skills/memo/lib/latest_phase.py`` (issue #473).

Issue #473: the ``<thread>.latest`` convenience-symlink convention was
consumer-maintained in spec terms but operationally load-bearing for
every consumer — and agent-invisible (the studio canary completed a
4-version PerfectCan iteration with zero symlinks). The fix ships a
runnable latest-phase CLI (the #472 ``render_phase.py`` fix shape)
wrapping the canonical writer
``anvil.lib.latest_resolution.update_latest_symlinks``, plus fenced
invocations in ``memo-draft.md`` step 9.6 / ``memo-revise.md`` step 9.8
/ ``memo-review.md`` step 12.5.

Covered per the curation test plan:

- CLI happy path via subprocess against copies of the existing
  ``tests/fixtures/latest_symlink/`` fixtures (walk-to-highest creates
  the symlink; pinned-symlink preserves the pin; ``--force`` re-points).
- Idempotence: second invocation on an unchanged thread dir is a no-op
  with a notice and exit 0.
- Missing / empty thread dir → notice + exit 0 (non-blocking contract).
- ``main()`` seam tests (injected fake writer) for the describe lines.
- Doc-contract assertions: the three lifecycle commands carry the
  fenced ``latest_phase.py`` invocation, the reviser's old
  "not touched" disclaimer is gone, and SKILL.md documents the
  framework-maintained contract.

The writer's behavioral corpus (pin discrimination, dangling repair,
real-dir refusal, per-family independence, ...) lives at
``tests/lib/test_latest_resolution.py`` next to the canonical module.

Per the #58 packaging convention, this file's filename
(``test_memo_latest_phase.py``) is unique across the
``anvil/skills/*/tests/`` tree.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import unittest
from io import StringIO
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

# Mirror test_memo_render_phase.py: import the skill-local lib module
# without a package install step.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
if str(_LIB) not in sys.path:
    sys.path.insert(0, str(_LIB))

import latest_phase  # noqa: E402

_REPO_ROOT = _HERE.parents[3]
_COMMANDS = _HERE.parent / "commands"
_FIXTURES = _HERE / "fixtures" / "latest_symlink"
_LATEST_PHASE_PY = _LIB / "latest_phase.py"


def _run_cli(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(_LATEST_PHASE_PY), *args],
        capture_output=True,
        text=True,
        cwd=str(_REPO_ROOT),
    )


def _copy_fixture(name: str, dest_root: Path) -> Path:
    """Copy a latest_symlink fixture thread into a temp root.

    ``symlinks=True`` preserves the pinned-symlink fixture's link as a
    link (not a dereferenced copy). The copied link's own mtime is then
    bumped to strictly after every version directory's mtime, so the pin
    classification is deterministic: ``update_latest_symlinks`` treats a
    link whose lstat mtime is >= the highest version dir's mtime as an
    intentional operator pin (set *after* the dirs existed), not a
    superseded tracking link. ``copytree`` copies the fixture's stored
    mtimes verbatim, so on a fresh checkout the version dirs and the
    thread root share a near-identical timestamp; anchoring the bump to
    ``max(version-dir mtime) + margin`` (rather than the thread-root
    mtime) makes the comparison unambiguous regardless of checkout time.
    """
    dest = dest_root / name
    shutil.copytree(_FIXTURES / name, dest, symlinks=True)
    version_mtimes = [
        child.stat().st_mtime
        for child in dest.iterdir()
        if child.is_dir() and not child.is_symlink()
    ]
    pin_mtime = (max(version_mtimes) if version_mtimes else os.stat(dest).st_mtime) + 10
    for child in dest.iterdir():
        if child.is_symlink():
            os.utime(child, (pin_mtime, pin_mtime), follow_symlinks=False)
    return dest


class CliFixtureTest(unittest.TestCase):
    """Subprocess runs against copies of the #288 fixtures."""

    def test_walk_to_highest_creates_symlink(self) -> None:
        with TemporaryDirectory() as td:
            thread = _copy_fixture("walk-to-highest", Path(td))
            result = _run_cli(str(thread))
            self.assertEqual(result.returncode, 0, result.stderr)
            link = thread / "walk-to-highest.latest"
            self.assertTrue(link.is_symlink())
            self.assertEqual(os.readlink(link), "walk-to-highest.3")
            self.assertIn("created", result.stdout)

    def test_pinned_symlink_is_preserved(self) -> None:
        with TemporaryDirectory() as td:
            thread = _copy_fixture("pinned-symlink", Path(td))
            result = _run_cli(str(thread))
            self.assertEqual(result.returncode, 0, result.stderr)
            link = thread / "pinned-symlink.latest"
            self.assertEqual(os.readlink(link), "pinned-symlink.2")
            self.assertIn("preserved", result.stdout)
            self.assertIn("--force", result.stdout)

    def test_force_repoints_pinned_symlink(self) -> None:
        with TemporaryDirectory() as td:
            thread = _copy_fixture("pinned-symlink", Path(td))
            result = _run_cli(str(thread), "--force")
            self.assertEqual(result.returncode, 0, result.stderr)
            link = thread / "pinned-symlink.latest"
            self.assertEqual(os.readlink(link), "pinned-symlink.3")
            self.assertIn("repointed", result.stdout)

    def test_second_invocation_is_noop_with_notice(self) -> None:
        with TemporaryDirectory() as td:
            thread = _copy_fixture("walk-to-highest", Path(td))
            first = _run_cli(str(thread))
            self.assertEqual(first.returncode, 0, first.stderr)
            second = _run_cli(str(thread))
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertIn("already up to date", second.stdout)
            link = thread / "walk-to-highest.latest"
            self.assertEqual(os.readlink(link), "walk-to-highest.3")

    def test_missing_thread_dir_exits_zero(self) -> None:
        with TemporaryDirectory() as td:
            result = _run_cli(str(Path(td) / "nope"))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("nothing to update", result.stdout)

    def test_empty_thread_dir_exits_zero(self) -> None:
        with TemporaryDirectory() as td:
            thread = Path(td) / "empty"
            thread.mkdir()
            result = _run_cli(str(thread))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("no version dirs", result.stdout)


class MainSeamTest(unittest.TestCase):
    """``main()`` with an injected fake writer (no framework import)."""

    def _main(self, argv, update_fn):
        out = StringIO()
        with mock.patch("sys.stdout", out):
            rc = latest_phase.main(argv, update_fn=update_fn)
        return rc, out.getvalue()

    def test_describe_lines_per_action(self) -> None:
        from anvil.lib.latest_resolution import LatestSymlinkUpdate

        updates = [
            LatestSymlinkUpdate("t.latest", "t.3", "created"),
            LatestSymlinkUpdate("t.latest.review", "t.2.review", "repointed", "repaired dangling symlink (was t.9.review)"),
            LatestSymlinkUpdate("t.latest.design", "t.3.design", "pinned", "pinned to t.1.design (non-highest); preserved — pass force=True to re-point"),
            LatestSymlinkUpdate("t.latest.audit", "t.3.audit", "refused-real-dir", "real directory exists at t.latest.audit; never replaced"),
        ]
        with TemporaryDirectory() as td:
            thread = Path(td) / "t"
            thread.mkdir()
            rc, out = self._main([str(thread)], lambda *a, **k: updates)
        self.assertEqual(rc, 0)
        self.assertIn("t.latest -> t.3 (created)", out)
        self.assertIn("repointed", out)
        self.assertIn("pass --force to re-point", out)
        self.assertIn("never replaced", out)

    def test_force_flag_threads_through(self) -> None:
        seen: dict = {}

        def fake(thread_dir, slug, *, force):
            seen["slug"] = slug
            seen["force"] = force
            return []

        with TemporaryDirectory() as td:
            thread = Path(td) / "acme"
            thread.mkdir()
            rc, _ = self._main([str(thread), "--force"], fake)
        self.assertEqual(rc, 0)
        self.assertEqual(seen["slug"], "acme")
        self.assertTrue(seen["force"])

    def test_writer_exception_is_non_blocking(self) -> None:
        def boom(*a, **k):
            raise RuntimeError("kaboom")

        with TemporaryDirectory() as td:
            thread = Path(td) / "acme"
            thread.mkdir()
            err = StringIO()
            with mock.patch("sys.stderr", err):
                rc = latest_phase.main([str(thread)], update_fn=boom)
        self.assertEqual(rc, 0)
        self.assertIn("non-blocking", err.getvalue())


class DocContractTest(unittest.TestCase):
    """The lifecycle wiring is present and the old disclaimers are gone."""

    INVOCATION = "python3 .anvil/skills/memo/lib/latest_phase.py <thread-dir>"

    def _read(self, path: Path) -> str:
        return path.read_text(encoding="utf-8")

    def test_memo_draft_step_9_6_invokes_cli(self) -> None:
        body = self._read(_COMMANDS / "memo-draft.md")
        self.assertIn("9.6.", body)
        self.assertIn(self.INVOCATION, body)

    def test_memo_revise_step_9_8_invokes_cli(self) -> None:
        body = self._read(_COMMANDS / "memo-revise.md")
        self.assertIn("9.8.", body)
        self.assertIn(self.INVOCATION, body)
        # The pre-#473 non-interaction disclaimer is retired.
        self.assertNotIn("symlinks are not touched", body)
        self.assertNotIn("Symlink maintenance is consumer-side", body)

    def test_memo_review_step_12_5_invokes_cli(self) -> None:
        body = self._read(_COMMANDS / "memo-review.md")
        self.assertIn("12.5.", body)
        self.assertIn(self.INVOCATION, body)

    def test_skill_md_documents_framework_maintained_contract(self) -> None:
        body = self._read(_HERE.parent / "SKILL.md")
        self.assertIn("framework-maintained by default", body)
        self.assertIn("latest_phase.py", body)
        self.assertIn("update_latest_symlinks", body)
        # The pre-#473 wording is gone.
        self.assertNotIn("maintenance is consumer-side", body)
        self.assertNotIn(
            "do not write, require, or read `.latest` symlinks", body
        )


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
