"""Integration tests for the portfolio-shared refs fixture (issue #280).

This test module pins the resolver's discovery contract against a
canary-shaped portfolio fixture under
``tests/fixtures/portfolio_shared_refs/``. The fixture mirrors the
Studio canary's 5-thread + sibling ``research/`` shape — five sibling
``anvil:memo`` threads (``investment-memo``, ``latency-wall``,
``technical-vision``, ``execution-plan``, ``team-thesis``) under one
portfolio dir, all sharing one body of evidence under
``research/00-intro.md``, ``research/01-market.md``,
``research/comps/silicon-comp-matrix.md``, and
``research/case-studies/acme-case-study.md``.

Scope:

- **Resolver discovery (AC6)**: assert that
  ``resolve_refs_dirs(thread_dir)`` returns BOTH the per-thread
  ``<thread>/refs/`` AND the portfolio-level
  ``<portfolio>/research/`` for every one of the five threads.
- **Per-thread precedence on filename collision**: assert that when
  both directories contain the same basename, the per-thread copy
  appears first in the returned list.
- **Citation-token convention**: assert that the documented
  ``[research/<file>]`` and ``[refs/<file>]`` citation tokens resolve
  correctly via the resolved list (the resolver returns directories;
  the test exercises iteration + basename-match per the documented
  caller responsibility).
- **Backwards compatibility (AC5)**: assert that a thread WITHOUT a
  sibling ``research/`` produces a one-entry resolved list (or empty,
  when neither ``refs/`` nor ``research/`` is present). The
  ``investment-memo`` thread under a synthetic parent (a tmp_path
  copy without ``research/``) exercises this shape.

Per the #58 packaging convention, this file's filename
(``test_portfolio_shared_refs_fixture.py``) is unique across the
``anvil/skills/*/tests/`` tree so the cross-skill pytest discovery
does not collide on basename.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import shutil
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


# Mirror test_anvil_config.py's sys.path injection so refs_resolver is
# importable as a top-level module without packaging-install gymnastics.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from refs_resolver import (  # noqa: E402
    REFS_DIRNAME,
    RESEARCH_DIRNAME,
    resolve_refs_dirs,
)


_FIXTURE_DIR = _HERE / "fixtures" / "portfolio_shared_refs"
_THREAD_NAMES = (
    "investment-memo",
    "latency-wall",
    "technical-vision",
    "execution-plan",
    "team-thesis",
)


class TestFixtureShape(unittest.TestCase):
    """The fixture exists on disk with the expected shape.

    These assertions are tied to AC6 of the curator's enhancement on
    issue #280 ("Canary repro fixture: a
    ``tests/fixtures/portfolio_shared_refs/`` directory mirroring the
    canary's 5-thread + sibling ``research/`` shape...").
    """

    def test_fixture_dir_exists(self) -> None:
        self.assertTrue(
            _FIXTURE_DIR.is_dir(),
            msg=f"fixture dir not found at {_FIXTURE_DIR}",
        )

    def test_all_five_thread_dirs_exist(self) -> None:
        for thread_name in _THREAD_NAMES:
            thread_dir = _FIXTURE_DIR / thread_name
            self.assertTrue(
                thread_dir.is_dir(),
                msg=f"thread dir not found: {thread_dir}",
            )

    def test_portfolio_research_dir_exists(self) -> None:
        research_dir = _FIXTURE_DIR / RESEARCH_DIRNAME
        self.assertTrue(
            research_dir.is_dir(),
            msg=f"portfolio research/ dir not found: {research_dir}",
        )

    def test_portfolio_research_has_canary_shaped_content(self) -> None:
        """The fixture's research/ pool mirrors the canary's shape.

        Specifically: vertical-brief markdown files at the research/
        root, a comps/ subdirectory with the silicon-comp-matrix, and a
        case-studies/ subdirectory.
        """
        research_dir = _FIXTURE_DIR / RESEARCH_DIRNAME
        # Vertical briefs at the root
        self.assertTrue((research_dir / "00-intro.md").is_file())
        self.assertTrue((research_dir / "01-market.md").is_file())
        # comps/ subdirectory with the silicon-comp-matrix
        self.assertTrue(
            (research_dir / "comps" / "silicon-comp-matrix.md").is_file()
        )
        # case-studies/ subdirectory
        self.assertTrue(
            (research_dir / "case-studies" / "acme-case-study.md").is_file()
        )

    def test_investment_memo_has_per_thread_refs(self) -> None:
        """The investment-memo thread has a per-thread refs file.

        The per-thread placement is intentional — it exercises the
        resolver's per-thread-first ordering contract by giving the
        investment-memo thread a unique refs entry that does NOT exist
        at the portfolio level.
        """
        thread_refs = _FIXTURE_DIR / "investment-memo" / REFS_DIRNAME
        self.assertTrue(thread_refs.is_dir())
        self.assertTrue((thread_refs / "cv-founder-jones.md").is_file())


# ---------------------------------------------------------------------------
# AC6 — resolver returns both dirs for every sibling thread
# ---------------------------------------------------------------------------


class TestResolverAgainstCanaryFixture(unittest.TestCase):
    """The resolver returns both dirs (per-thread + portfolio) for every thread.

    This is the load-bearing AC6 assertion: the shared evidence pool is
    discoverable from any sibling thread, not just one.
    """

    def test_every_thread_resolves_to_both_dirs(self) -> None:
        for thread_name in _THREAD_NAMES:
            with self.subTest(thread=thread_name):
                thread_dir = _FIXTURE_DIR / thread_name
                result = resolve_refs_dirs(thread_dir)

                self.assertEqual(
                    len(result),
                    2,
                    msg=(
                        f"thread {thread_name!r}: expected 2 resolved "
                        f"dirs, got {[str(p) for p in result]}"
                    ),
                )
                self.assertEqual(result[0], thread_dir / REFS_DIRNAME)
                self.assertEqual(
                    result[1], _FIXTURE_DIR / RESEARCH_DIRNAME
                )

    def test_per_thread_refs_first_for_every_thread(self) -> None:
        """The per-thread refs/ entry is always [0] in the returned list.

        This is the load-bearing precedence contract: a caller that
        iterates the resolved list and picks-first on basename gets the
        per-thread copy when one exists.
        """
        for thread_name in _THREAD_NAMES:
            with self.subTest(thread=thread_name):
                thread_dir = _FIXTURE_DIR / thread_name
                result = resolve_refs_dirs(thread_dir)
                self.assertEqual(result[0].name, REFS_DIRNAME)
                self.assertEqual(result[1].name, RESEARCH_DIRNAME)


# ---------------------------------------------------------------------------
# Citation-token convention — [refs/<file>] and [research/<file>]
# ---------------------------------------------------------------------------


class TestCitationTokenResolution(unittest.TestCase):
    """The documented citation tokens resolve via the resolved list.

    The drafter / reviewer convention extension from issue #280 is:

    - ``[refs/<file>]`` resolves via the per-thread directory.
    - ``[research/<file>]`` resolves via the portfolio-level directory.

    The resolver returns the directories; this test exercises the
    iteration + basename-match logic the drafter / reviewer perform in
    caller code, against the fixture.
    """

    def _resolve_citation(
        self, thread_dir: Path, dir_name: str, file_rel: str
    ) -> Path | None:
        """Mirror the documented caller-side resolution.

        Iterate the resolved list, pick the FIRST entry whose basename
        matches ``dir_name``, and look up ``file_rel`` relative to that
        entry. Returns the resolved Path when the file exists, or
        ``None`` when the citation cannot be resolved (no matching
        directory, or file absent).

        This mirrors the convention surfaced in
        ``commands/memo-review.md`` step 5 (the dim-3 refs back-check
        sub-step) and in ``commands/memo-draft.md`` step 3 (the
        drafter's source-of-truth ingestion).
        """
        for resolved_dir in resolve_refs_dirs(thread_dir):
            if resolved_dir.name == dir_name:
                candidate = resolved_dir / file_rel
                if candidate.is_file():
                    return candidate
        return None

    def test_research_token_resolves_for_every_thread(self) -> None:
        """A ``[research/comps/silicon-comp-matrix.md]`` cite resolves from every thread.

        This is the load-bearing canary repro: today (pre-#280) this
        cite resolves from no thread because the back-check only reads
        ``<thread>/refs/``; with the resolver, it resolves from every
        thread because the portfolio-level ``research/`` is appended.
        """
        for thread_name in _THREAD_NAMES:
            with self.subTest(thread=thread_name):
                thread_dir = _FIXTURE_DIR / thread_name
                resolved = self._resolve_citation(
                    thread_dir,
                    RESEARCH_DIRNAME,
                    "comps/silicon-comp-matrix.md",
                )
                self.assertIsNotNone(
                    resolved,
                    msg=(
                        f"thread {thread_name!r}: research-pool cite did "
                        f"not resolve (issue #280 canary repro broken)"
                    ),
                )
                # Sanity: the resolved file is under the portfolio
                # research/ pool, not under any thread's refs/.
                self.assertEqual(
                    resolved,
                    _FIXTURE_DIR
                    / RESEARCH_DIRNAME
                    / "comps"
                    / "silicon-comp-matrix.md",
                )

    def test_refs_token_resolves_for_investment_memo(self) -> None:
        """A ``[refs/cv-founder-jones.md]`` cite resolves via the per-thread pool.

        Pre-#280 behavior preservation — the per-thread `refs/`
        directory is still the first lookup target.
        """
        thread_dir = _FIXTURE_DIR / "investment-memo"
        resolved = self._resolve_citation(
            thread_dir, REFS_DIRNAME, "cv-founder-jones.md"
        )
        self.assertIsNotNone(resolved)
        self.assertEqual(
            resolved, thread_dir / REFS_DIRNAME / "cv-founder-jones.md"
        )

    def test_refs_token_does_not_leak_across_threads(self) -> None:
        """A per-thread refs file is NOT visible from sibling threads.

        The investment-memo thread's `cv-founder-jones.md` is at
        `investment-memo/refs/cv-founder-jones.md` — it MUST NOT
        appear when resolving from any sibling thread's perspective.
        """
        for thread_name in (
            "latency-wall",
            "technical-vision",
            "execution-plan",
            "team-thesis",
        ):
            with self.subTest(thread=thread_name):
                thread_dir = _FIXTURE_DIR / thread_name
                resolved = self._resolve_citation(
                    thread_dir, REFS_DIRNAME, "cv-founder-jones.md"
                )
                self.assertIsNone(
                    resolved,
                    msg=(
                        f"thread {thread_name!r}: investment-memo's "
                        f"per-thread CV unexpectedly resolved — refs/ "
                        f"is per-thread, not shared"
                    ),
                )


# ---------------------------------------------------------------------------
# AC5 — backwards compatibility (no sibling research/)
# ---------------------------------------------------------------------------


class TestBackwardsCompatRegression(unittest.TestCase):
    """A thread WITHOUT a sibling research/ behaves byte-identically to pre-#280.

    This is the load-bearing AC5 assertion: the resolver returns
    ``[<thread>/refs/]`` (or ``[]`` when refs/ is also absent) and
    nothing else when no sibling research/ exists. The test copies the
    investment-memo thread into a tmp_path WITHOUT the research/
    sibling to exercise this shape.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.tmp_portfolio = Path(self._td.name) / "no-research-portfolio"
        self.tmp_portfolio.mkdir(parents=True, exist_ok=True)
        # Copy just the investment-memo thread (with its refs/), NOT
        # the sibling research/ pool.
        shutil.copytree(
            _FIXTURE_DIR / "investment-memo",
            self.tmp_portfolio / "investment-memo",
        )
        self.addCleanup(self._td.cleanup)

    def test_resolver_returns_only_thread_refs_when_no_research(self) -> None:
        thread_dir = self.tmp_portfolio / "investment-memo"
        result = resolve_refs_dirs(thread_dir)
        self.assertEqual(result, [thread_dir / REFS_DIRNAME])

    def test_refs_cite_still_resolves_when_no_research(self) -> None:
        """Pre-#280 behavior preserved: per-thread refs cite resolves normally."""
        thread_dir = self.tmp_portfolio / "investment-memo"
        candidate = thread_dir / REFS_DIRNAME / "cv-founder-jones.md"
        self.assertTrue(
            candidate.is_file(),
            msg="copytree should have copied the per-thread refs file",
        )
        # The cite resolves via the per-thread dir as it always has.
        for resolved_dir in resolve_refs_dirs(thread_dir):
            if (resolved_dir / "cv-founder-jones.md").is_file():
                self.assertEqual(resolved_dir, thread_dir / REFS_DIRNAME)
                return
        self.fail("per-thread cite failed to resolve in no-research portfolio")

    def test_research_cite_does_not_resolve_when_no_research(self) -> None:
        """A research-pool cite cannot resolve when no portfolio research/ exists.

        Documents the inverse of the canary repro: without the
        portfolio-level pool, citations to it cannot resolve. This is
        the pre-#280 status quo and is preserved.
        """
        thread_dir = self.tmp_portfolio / "investment-memo"
        for resolved_dir in resolve_refs_dirs(thread_dir):
            self.assertNotEqual(
                resolved_dir.name,
                RESEARCH_DIRNAME,
                msg=(
                    "resolver returned a research/ dir for a portfolio "
                    "that has no sibling research/ — bug"
                ),
            )


if __name__ == "__main__":
    unittest.main()
