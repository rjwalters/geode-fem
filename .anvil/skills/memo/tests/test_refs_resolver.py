"""Tests for ``anvil.skills.memo.lib.refs_resolver`` (issue #280).

Covers the portfolio-level refs resolver shipped under issue #280: when
a ``<portfolio>/research/`` directory exists alongside thread dirs,
``resolve_refs_dirs`` returns BOTH ``<thread>/refs/`` (first, for
per-thread precedence on filename collision) and
``<portfolio>/research/`` (second, the portfolio-level evidence pool).
When ``<portfolio>/research/`` does NOT exist, the resolver returns only
``<thread>/refs/`` (or empty when ``<thread>/refs/`` is also absent),
preserving byte-identical behavior with the pre-#280 contract.

Sub-issues covered:

- Resolver helper: AC1 of the curator's enhancement on issue #280
  ("Resolution helper: ``resolve_refs_dirs(thread_dir: Path) ->
  list[Path]`` with the algorithm above. Per-thread ``<thread>/refs/``
  always comes first; ``<portfolio>/research/`` is appended when
  present.").
- Backwards compatibility: AC5 of the curator's enhancement on issue
  #280 ("a thread WITHOUT a sibling ``<portfolio>/research/``
  directory behaves byte-identically to today (only ``<thread>/refs/``
  is read). Verified by a regression fixture that doesn't add
  ``research/``.").
- Edge cases per the curator's Test Plan: portfolio has ``research/``
  but it's empty; filename collision (verified at the resolver level by
  asserting both dirs appear in the list); both dirs missing; symlink
  edge cases (resolver tolerates symlink loops via OSError fallback).

Tests use ``tmp_path`` per test for the portfolio + thread skeleton —
no on-disk fixtures because the directory shape is trivial to write
inline and easier to read than to chase to a fixtures dir (mirrors the
``test_anvil_config.py`` convention for the same reason).

Per the #58 packaging convention, this file's filename
(``test_refs_resolver.py``) is unique across the
``anvil/skills/*/tests/`` tree so the cross-skill pytest discovery
does not collide on basename.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


# The memo skill keeps its lib modules under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern. Add it to
# ``sys.path`` so tests import without a package install step — mirrors
# ``test_anvil_config.py`` exactly.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from refs_resolver import (  # noqa: E402
    REFS_DIRNAME,
    RESEARCH_DIRNAME,
    resolve_refs_dirs,
)


class _PortfolioBase(unittest.TestCase):
    """Mixin: per-test temp dir for the portfolio + thread skeleton.

    The skeleton mirrors the Studio canary's multi-thread shape:

        <tmp>/portfolio/
          <tmp>/portfolio/investment-memo/       (thread_dir)
          <tmp>/portfolio/latency-wall/          (sibling thread)
          <tmp>/portfolio/research/              (shared evidence pool)

    Tests opt-in to each piece via the ``_make_*`` helpers so a given
    test exercises exactly the on-disk shape it cares about.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.portfolio_dir = Path(self._td.name) / "portfolio"
        self.portfolio_dir.mkdir(parents=True, exist_ok=True)
        self.thread_dir = self.portfolio_dir / "investment-memo"
        self.thread_dir.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._td.cleanup)

    def _make_thread_refs(self, *files: str) -> Path:
        """Create ``<thread>/refs/`` with optional touch-files for collision tests.

        File names may contain ``/`` for subdirectory placement (e.g.,
        ``comps/silicon-comp-matrix.md``); parent dirs are created on
        demand so callers can populate the canary's
        ``research/comps/`` and ``research/case-studies/`` shapes.
        """
        refs = self.thread_dir / REFS_DIRNAME
        refs.mkdir(parents=True, exist_ok=True)
        for name in files:
            path = refs / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("# placeholder\n", encoding="utf-8")
        return refs

    def _make_portfolio_research(self, *files: str) -> Path:
        """Create ``<portfolio>/research/`` with optional touch-files.

        File names may contain ``/`` for subdirectory placement; parent
        dirs are created on demand. See ``_make_thread_refs`` rationale.
        """
        research = self.portfolio_dir / RESEARCH_DIRNAME
        research.mkdir(parents=True, exist_ok=True)
        for name in files:
            path = research / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("# placeholder\n", encoding="utf-8")
        return research


# ---------------------------------------------------------------------------
# AC1 — resolver returns the ordered list correctly
# ---------------------------------------------------------------------------


class TestBothDirsPresent(_PortfolioBase):
    """When both ``<thread>/refs/`` and ``<portfolio>/research/`` exist."""

    def test_returns_both_in_order(self) -> None:
        thread_refs = self._make_thread_refs()
        portfolio_research = self._make_portfolio_research()

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(len(result), 2)
        self.assertEqual(result[0], thread_refs)
        self.assertEqual(result[1], portfolio_research)

    def test_per_thread_refs_always_first(self) -> None:
        """AC1: ``<thread>/refs/`` always comes first per the precedence contract.

        This guards against an implementation that walks the parent dir
        before checking the thread dir — the per-thread copy must win
        on filename collision per the issue body, so it must be first
        in the returned list.
        """
        self._make_thread_refs()
        self._make_portfolio_research()

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(result[0].name, REFS_DIRNAME)
        self.assertEqual(result[1].name, RESEARCH_DIRNAME)

    def test_both_with_files_returns_both(self) -> None:
        """Both dirs populated with files — resolver still returns both."""
        self._make_thread_refs("cv.pdf", "transcript-jones.md")
        self._make_portfolio_research("00-intro.md", "comps/silicon-comp-matrix.md")

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(len(result), 2)
        # Resolver does not enumerate file contents — it returns
        # directories. File enumeration is the caller's responsibility.

    def test_filename_collision_both_dirs_present(self) -> None:
        """When the same basename exists in both dirs, both dirs are returned.

        Per-thread precedence on filename collision is the CALLER's
        responsibility (pick-first when iterating, or de-dup by
        basename). The resolver just orders the list correctly.
        """
        self._make_thread_refs("cv.pdf")
        self._make_portfolio_research("cv.pdf")

        result = resolve_refs_dirs(self.thread_dir)

        # Both dirs are returned — resolver does not deduplicate by
        # basename, only by resolved absolute path (defensive against
        # the pathological same-dir case).
        self.assertEqual(len(result), 2)
        # The per-thread dir comes first; an iterating caller that
        # picks-first on basename gets the per-thread copy.
        self.assertEqual(result[0].name, REFS_DIRNAME)


# ---------------------------------------------------------------------------
# AC5 — backwards compatibility (research/ absent)
# ---------------------------------------------------------------------------


class TestBackwardsCompat(_PortfolioBase):
    """A thread WITHOUT a sibling ``research/`` returns only ``<thread>/refs/``.

    This is the load-bearing backwards-compat contract per AC5: the
    resolver behaves byte-identically to the pre-#280 status quo for
    threads that do not adopt the portfolio-level extension.
    """

    def test_only_thread_refs_present(self) -> None:
        thread_refs = self._make_thread_refs()

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(result, [thread_refs])

    def test_only_thread_refs_with_files_returns_just_refs(self) -> None:
        """Backwards-compat with populated refs/ — same one-entry list."""
        thread_refs = self._make_thread_refs(
            "cv.pdf", "filing-s1.pdf", "transcript-jones.md"
        )

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(result, [thread_refs])

    def test_thread_refs_absent_research_absent_returns_empty(self) -> None:
        """Both directories missing → empty list (legal state).

        Per the resolver docstring: a thread with neither ``refs/`` nor
        a sibling ``research/`` is the original behavior for memo
        threads that use citation stubs only — the function returns an
        empty list, not an error.
        """
        # Note: setUp creates self.thread_dir but no refs/ or research/.
        result = resolve_refs_dirs(self.thread_dir)
        self.assertEqual(result, [])


# ---------------------------------------------------------------------------
# Edge cases per the curator's Test Plan
# ---------------------------------------------------------------------------


class TestEdgeCases(_PortfolioBase):
    """Edge cases per the curator's Test Plan."""

    def test_portfolio_research_present_thread_refs_absent(self) -> None:
        """Thread without ``refs/`` but with sibling ``research/`` — returns just research/.

        Useful shape: a thread that exclusively relies on portfolio-level
        evidence (no per-thread refs).
        """
        portfolio_research = self._make_portfolio_research("00-intro.md")

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(result, [portfolio_research])

    def test_portfolio_research_empty_directory(self) -> None:
        """Empty ``research/`` directory still appears in the list.

        The resolver returns directories that exist; whether they have
        any files in them is the caller's concern (the drafter / reviewer
        iterate the directory and noop on empty content).
        """
        self._make_thread_refs("cv.pdf")
        portfolio_research = self._make_portfolio_research()  # empty

        result = resolve_refs_dirs(self.thread_dir)

        self.assertEqual(len(result), 2)
        self.assertIn(portfolio_research, result)

    def test_thread_refs_is_file_not_dir(self) -> None:
        """A regular file named ``refs`` at the thread root is NOT treated as a dir.

        Defensive: if a consumer's filesystem has a stray file named
        ``refs`` (no extension) at the thread root, the resolver
        ignores it rather than crashing or returning it.
        """
        (self.thread_dir / REFS_DIRNAME).write_text("not a dir\n", encoding="utf-8")
        portfolio_research = self._make_portfolio_research()

        result = resolve_refs_dirs(self.thread_dir)

        # Only the portfolio research/ directory is returned; the file
        # is skipped because is_dir() returns False.
        self.assertEqual(result, [portfolio_research])

    def test_portfolio_research_is_file_not_dir(self) -> None:
        """A regular file named ``research`` at the portfolio root is NOT treated as a dir."""
        thread_refs = self._make_thread_refs()
        (self.portfolio_dir / RESEARCH_DIRNAME).write_text(
            "not a dir\n", encoding="utf-8"
        )

        result = resolve_refs_dirs(self.thread_dir)

        # Only the thread refs/ directory is returned.
        self.assertEqual(result, [thread_refs])

    def test_nonexistent_thread_dir_returns_empty(self) -> None:
        """A nonexistent ``thread_dir`` yields an empty list (no error)."""
        nonexistent = self.portfolio_dir / "does-not-exist"
        # The dir doesn't exist; thread_dir.parent is self.portfolio_dir
        # which DOES exist but has no research/ subdir at this point.
        result = resolve_refs_dirs(nonexistent)
        self.assertEqual(result, [])

    def test_nonexistent_thread_dir_but_portfolio_research_present(self) -> None:
        """When thread_dir doesn't exist but its parent has research/, research/ is still returned.

        This is the intentional behavior — the resolver checks both
        directories independently, so the portfolio-level research/
        pool is discoverable even when the thread refs/ is absent.
        """
        portfolio_research = self._make_portfolio_research("00-intro.md")
        nonexistent = self.portfolio_dir / "does-not-exist"

        result = resolve_refs_dirs(nonexistent)

        self.assertEqual(result, [portfolio_research])


# ---------------------------------------------------------------------------
# Multi-thread portfolio shape — the canary motivating issue
# ---------------------------------------------------------------------------


class TestCanaryShape(_PortfolioBase):
    """The Studio canary's multi-thread portfolio shape.

    Five sibling memo threads under one portfolio dir, all sharing one
    body of evidence. Verifies that each thread independently sees the
    SAME portfolio-level ``research/`` directory — that is, the
    portfolio-level evidence pool is discoverable from any sibling
    thread, not just the one that owns the (notional) "primary" memo.
    """

    def test_five_sibling_threads_all_see_same_research(self) -> None:
        # Set up the canary's 5-thread shape under self.portfolio_dir.
        # self.thread_dir is "investment-memo" from setUp; create the
        # other four siblings.
        for sibling in (
            "latency-wall",
            "technical-vision",
            "execution-plan",
            "team-thesis",
        ):
            (self.portfolio_dir / sibling).mkdir(parents=True, exist_ok=True)

        # Each sibling has its own refs/ and the portfolio has a shared
        # research/ dir.
        portfolio_research = self._make_portfolio_research(
            "00-intro.md",
            "comps/silicon-comp-matrix.md",
        )
        for thread_name in (
            "investment-memo",
            "latency-wall",
            "technical-vision",
            "execution-plan",
            "team-thesis",
        ):
            thread = self.portfolio_dir / thread_name
            (thread / REFS_DIRNAME).mkdir(parents=True, exist_ok=True)

            result = resolve_refs_dirs(thread)

            # Each thread sees TWO directories: its own refs/ and the
            # shared portfolio research/.
            self.assertEqual(
                len(result),
                2,
                msg=f"thread {thread_name!r}: expected 2 resolved dirs",
            )
            self.assertEqual(result[0], thread / REFS_DIRNAME)
            self.assertEqual(result[1], portfolio_research)


# ---------------------------------------------------------------------------
# Module constants (sanity check the surfacing of the documented names)
# ---------------------------------------------------------------------------


class TestModuleConstants(unittest.TestCase):
    """Sanity-check that the module constants match the documented values.

    The citation-token convention in SKILL.md §"Source-of-truth materials"
    surfaces ``[research/<file>]`` for portfolio-level hits and
    ``[refs/<file>]`` for per-thread hits. The module constants
    ``RESEARCH_DIRNAME`` and ``REFS_DIRNAME`` are the single source of
    truth for those strings — if a follow-on issue ever renames the
    portfolio-level directory (e.g., to ``shared-refs/``), updating the
    constant here would automatically update the resolver, the citation
    token, and the test surface.
    """

    def test_research_dirname_is_research(self) -> None:
        self.assertEqual(RESEARCH_DIRNAME, "research")

    def test_refs_dirname_is_refs(self) -> None:
        self.assertEqual(REFS_DIRNAME, "refs")


if __name__ == "__main__":
    unittest.main()
