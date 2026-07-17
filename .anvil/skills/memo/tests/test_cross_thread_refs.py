"""Tests for ``anvil.skills.memo.lib.cross_thread_refs`` (issue #287).

Covers the cross-thread reference resolver shipped under issue #287
(sub-deliverable 4 of #283): native validation of
``[[../<other-slug>/<other-slug>.latest]]`` and
``[[../<other-slug>/<other-slug>.N]]`` references in ``memo.md`` for the
reviewer's dim-3 back-check.

Sub-issues covered:

- **Parser**: enumerate cross-thread refs from a markdown body.
  Handles ``.latest``, explicit ``.N``, file suffix
  (``/<file>`` and ``/exhibits/<file>``), multiple refs per line,
  multiple refs across lines.
- **Resolver — resolved cases**: refs that resolve cleanly to
  (a) a version dir without a file suffix, (b) a version dir's
  ``memo.md``, (c) an ``exhibits/<file>`` under a version dir.
- **Resolver — unresolved cases**: missing thread, missing version
  (typo'd N), missing file inside an otherwise-valid version dir,
  unresolvable ``.latest`` (no symlink + no ``<slug>.N/`` siblings).
- **`.latest` tolerance**: symlink form, real-directory form named
  ``.latest``, walk-to-highest fallback when neither exists.

Per the #58 packaging convention, this file's filename
(``test_cross_thread_refs.py``) is unique across the
``anvil/skills/*/tests/`` tree so the cross-skill pytest discovery does
not collide on basename. The companion fixture lives at
``tests/fixtures/cross_thread_refs/`` (also distinct from the existing
``cross_thread_cite_consistency/`` (issue #236) and ``project_brief/``
(issue #284) fixtures — verified by direct inspection of the fixtures
directory).

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


# Mirror test_refs_resolver.py's sys.path injection — the memo skill
# keeps its lib modules under its own ``lib/`` per CLAUDE.md "skill-local
# first, lib promotion later" and we want to import without packaging
# install gymnastics.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from cross_thread_refs import (  # noqa: E402
    LATEST,
    CrossThreadRef,
    CrossThreadResolution,
    find_cross_thread_refs,
    resolve_cross_thread_ref,
    resolve_cross_thread_refs,
)


_FIXTURE_DIR = _HERE / "fixtures" / "cross_thread_refs"


# ---------------------------------------------------------------------------
# Parser — find_cross_thread_refs
# ---------------------------------------------------------------------------


class TestParser(unittest.TestCase):
    """The parser correctly enumerates the four shipped reference shapes."""

    def test_empty_body_returns_empty_list(self) -> None:
        """No refs in body → empty list (backwards-compat anchor).

        A memo with no cross-thread refs gets byte-identical pre-#287
        behavior. This is the load-bearing backwards-compat contract.
        """
        self.assertEqual(find_cross_thread_refs(""), [])
        self.assertEqual(find_cross_thread_refs("Just plain prose."), [])
        self.assertEqual(
            find_cross_thread_refs("A `[link](http://example.com)` but no cross-thread refs."),
            [],
        )

    def test_latest_without_file(self) -> None:
        """``[[../slug/slug.latest]]`` — version dir, no file suffix."""
        text = "See [[../brasidas-synthesis/brasidas-synthesis.latest]] for context."
        refs = find_cross_thread_refs(text)

        self.assertEqual(len(refs), 1)
        ref = refs[0]
        self.assertEqual(ref.line, 1)
        self.assertEqual(ref.other_slug, "brasidas-synthesis")
        self.assertEqual(ref.version, LATEST)
        self.assertIsNone(ref.file)
        self.assertIn("brasidas-synthesis.latest", ref.raw)

    def test_explicit_version_without_file(self) -> None:
        """``[[../slug/slug.N]]`` — explicit version, no file suffix."""
        text = "Pinned to [[../latency-wall/latency-wall.3]]."
        refs = find_cross_thread_refs(text)

        self.assertEqual(len(refs), 1)
        ref = refs[0]
        self.assertEqual(ref.other_slug, "latency-wall")
        self.assertEqual(ref.version, "3")
        self.assertIsNone(ref.file)

    def test_latest_with_memo_md_suffix(self) -> None:
        """``[[../slug/slug.latest/memo.md]]`` — body file."""
        text = "See [[../investment-memo/investment-memo.latest/memo.md]] §2."
        refs = find_cross_thread_refs(text)

        self.assertEqual(len(refs), 1)
        ref = refs[0]
        self.assertEqual(ref.other_slug, "investment-memo")
        self.assertEqual(ref.version, LATEST)
        self.assertEqual(ref.file, "memo.md")

    def test_explicit_version_with_exhibits_file(self) -> None:
        """``[[../slug/slug.N/exhibits/<file>]]`` — exhibit artifact."""
        text = "See [[../technical-vision/technical-vision.2/exhibits/figure-arch.png]]."
        refs = find_cross_thread_refs(text)

        self.assertEqual(len(refs), 1)
        ref = refs[0]
        self.assertEqual(ref.other_slug, "technical-vision")
        self.assertEqual(ref.version, "2")
        self.assertEqual(ref.file, "exhibits/figure-arch.png")

    def test_multiple_refs_same_line(self) -> None:
        """Two refs on one line are both enumerated in source order."""
        text = (
            "Compare [[../investment-memo/investment-memo.latest]] with "
            "[[../latency-wall/latency-wall.3]]."
        )
        refs = find_cross_thread_refs(text)

        self.assertEqual(len(refs), 2)
        self.assertEqual(refs[0].other_slug, "investment-memo")
        self.assertEqual(refs[0].version, LATEST)
        self.assertEqual(refs[1].other_slug, "latency-wall")
        self.assertEqual(refs[1].version, "3")
        # Both on line 1.
        self.assertEqual(refs[0].line, 1)
        self.assertEqual(refs[1].line, 1)

    def test_multiple_refs_different_lines(self) -> None:
        """Refs on different lines get their 1-based line numbers."""
        text = (
            "Line 1 has [[../investment-memo/investment-memo.1]].\n"
            "Line 2 plain text.\n"
            "Line 3 has [[../latency-wall/latency-wall.latest/memo.md]].\n"
        )
        refs = find_cross_thread_refs(text)

        self.assertEqual(len(refs), 2)
        self.assertEqual(refs[0].line, 1)
        self.assertEqual(refs[1].line, 3)
        self.assertEqual(refs[1].file, "memo.md")

    def test_mismatched_slug_does_not_match(self) -> None:
        """``[[../a/b.latest]]`` (slug mismatch in path) is NOT a valid ref.

        The shipped convention is ``[[../<slug>/<slug>.N]]`` — the slug
        is repeated as the stem of the version dir. A path that uses one
        slug and a different stem is a typo / different convention and
        the parser does NOT claim it. (Resolution would fail anyway
        because no such on-disk shape exists; rejecting at parse time
        avoids surfacing a misleading "thread not found" finding for a
        non-anvil link.)
        """
        text = "See [[../a/b.latest]]."
        refs = find_cross_thread_refs(text)
        self.assertEqual(refs, [])

    def test_no_cross_thread_anchor_does_not_match(self) -> None:
        """A wiki-link without the ``../`` cross-thread anchor is ignored.

        ``[[same-thread/same-thread.1]]`` is not a cross-thread ref —
        it's intra-thread (and not a documented anvil convention). The
        parser is anchored on ``../`` to avoid pulling in arbitrary
        wiki-style links.
        """
        text = "See [[brasidas-synthesis/brasidas-synthesis.latest]]."
        refs = find_cross_thread_refs(text)
        self.assertEqual(refs, [])

    def test_underscored_slug_matches(self) -> None:
        """Slugs with underscores match — slug shape allows alnum/-/_."""
        text = "See [[../my_thread/my_thread.latest]]."
        refs = find_cross_thread_refs(text)
        self.assertEqual(len(refs), 1)
        self.assertEqual(refs[0].other_slug, "my_thread")


# ---------------------------------------------------------------------------
# Resolver — resolved cases
# ---------------------------------------------------------------------------


class _ResolverBase(unittest.TestCase):
    """Per-test temp dir for portfolio + sibling-thread skeleton.

    Provides ``self.portfolio_dir`` (the parent under which sibling
    threads live), ``self.thread_dir`` (the citing thread root), and
    helper methods to populate sibling threads with version directories
    and files.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.portfolio_dir = Path(self._td.name) / "portfolio"
        self.portfolio_dir.mkdir(parents=True, exist_ok=True)
        # The citing thread isn't necessary for resolver tests (the
        # resolver only uses ``portfolio_root``), but we create one for
        # realism so callers see the canary's multi-thread shape.
        self.thread_dir = self.portfolio_dir / "investment-memo"
        self.thread_dir.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._td.cleanup)

    def _make_sibling_thread(
        self, slug: str, *version_specs: str
    ) -> Path:
        """Create ``<portfolio>/<slug>/`` with one or more version dirs.

        ``version_specs`` is a list of strings — each either:

        - An integer string (e.g., ``"1"``, ``"3"``) → creates
          ``<portfolio>/<slug>/<slug>.<N>/``.
        - The string ``"latest:N"`` (e.g., ``"latest:3"``) → creates
          ``<portfolio>/<slug>/<slug>.<N>/`` AND a symlink
          ``<portfolio>/<slug>/<slug>.latest`` pointing at it.
        - The string ``"latest-real"`` → creates a REAL directory at
          ``<portfolio>/<slug>/<slug>.latest`` (not a symlink).

        Returns the sibling thread root path.
        """
        sibling_dir = self.portfolio_dir / slug
        sibling_dir.mkdir(parents=True, exist_ok=True)
        for spec in version_specs:
            if spec.startswith("latest:"):
                n = spec.split(":", 1)[1]
                target = sibling_dir / f"{slug}.{n}"
                target.mkdir(parents=True, exist_ok=True)
                symlink = sibling_dir / f"{slug}.{LATEST}"
                # Use a relative symlink so the fixture is filesystem-
                # location-independent.
                os.symlink(target.name, symlink)
            elif spec == "latest-real":
                target = sibling_dir / f"{slug}.{LATEST}"
                target.mkdir(parents=True, exist_ok=True)
            else:
                # Numeric version spec.
                target = sibling_dir / f"{slug}.{spec}"
                target.mkdir(parents=True, exist_ok=True)
        return sibling_dir

    def _touch(self, path: Path, content: str = "# placeholder\n") -> Path:
        """Touch a file (creating parent dirs)."""
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path


class TestResolverResolved(_ResolverBase):
    """Refs that resolve cleanly to existing on-disk paths."""

    def test_resolved_explicit_version_dir(self) -> None:
        """``[[../slug/slug.N]]`` resolves to ``<portfolio>/<slug>/<slug>.<N>/``."""
        self._make_sibling_thread("latency-wall", "3")

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.3]]",
            other_slug="latency-wall", version="3", file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertTrue(result.resolved)
        self.assertIsNone(result.reason)
        self.assertEqual(
            result.target_path,
            self.portfolio_dir / "latency-wall" / "latency-wall.3",
        )

    def test_resolved_latest_via_symlink(self) -> None:
        """``[[../slug/slug.latest]]`` resolves via ``.latest`` symlink."""
        self._make_sibling_thread("latency-wall", "latest:3")

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.latest]]",
            other_slug="latency-wall", version=LATEST, file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertTrue(result.resolved)
        # The symlink target IS ``<slug>.latest`` (we don't resolve
        # symlinks — both forms are valid).
        self.assertEqual(
            result.target_path,
            self.portfolio_dir / "latency-wall" / "latency-wall.latest",
        )

    def test_resolved_latest_via_real_directory(self) -> None:
        """``[[../slug/slug.latest]]`` resolves via a REAL ``.latest/`` dir.

        Sub-deliverable 5 (#288) ships the symlink convention; this test
        confirms that this module also works when the operator has set
        up a real directory at ``.latest`` (or hasn't migrated yet).
        """
        self._make_sibling_thread("latency-wall", "latest-real")

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.latest]]",
            other_slug="latency-wall", version=LATEST, file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertTrue(result.resolved)
        self.assertEqual(
            result.target_path,
            self.portfolio_dir / "latency-wall" / "latency-wall.latest",
        )

    def test_resolved_latest_via_walk_to_highest(self) -> None:
        """``.latest`` falls back to walking ``<slug>.N/`` and picking highest.

        Works today even without sub-deliverable 5's ``.latest`` symlink
        convention. The fallback is permissive on purpose.
        """
        # Three version dirs, no .latest. Highest is 3.
        self._make_sibling_thread("latency-wall", "1", "2", "3")

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.latest]]",
            other_slug="latency-wall", version=LATEST, file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertTrue(result.resolved)
        self.assertEqual(
            result.target_path,
            self.portfolio_dir / "latency-wall" / "latency-wall.3",
        )

    def test_resolved_memo_md_inside_version_dir(self) -> None:
        """``[[../slug/slug.N/memo.md]]`` resolves to the body file."""
        sibling = self._make_sibling_thread("latency-wall", "3")
        self._touch(sibling / "latency-wall.3" / "memo.md", "# memo body\n")

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.3/memo.md]]",
            other_slug="latency-wall", version="3", file="memo.md",
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertTrue(result.resolved)
        self.assertEqual(
            result.target_path,
            self.portfolio_dir / "latency-wall" / "latency-wall.3" / "memo.md",
        )

    def test_resolved_exhibits_file_inside_version_dir(self) -> None:
        """``[[../slug/slug.N/exhibits/<file>]]`` resolves to an exhibit."""
        sibling = self._make_sibling_thread("technical-vision", "2")
        self._touch(
            sibling / "technical-vision.2" / "exhibits" / "figure-arch.png",
            "fake png bytes",
        )

        ref = CrossThreadRef(
            line=1,
            raw="[[../technical-vision/technical-vision.2/exhibits/figure-arch.png]]",
            other_slug="technical-vision", version="2",
            file="exhibits/figure-arch.png",
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertTrue(result.resolved)


# ---------------------------------------------------------------------------
# Resolver — unresolved cases (the load-bearing failure modes)
# ---------------------------------------------------------------------------


class TestResolverUnresolved(_ResolverBase):
    """Refs whose resolution fails — the dim-3 deduction surface."""

    def test_unresolved_thread_not_found(self) -> None:
        """Sibling thread dir does not exist → ``"thread not found"``."""
        # Don't create any sibling threads. ``investment-memo`` (the
        # citing thread from setUp) exists but ``latency-wall`` doesn't.
        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.3]]",
            other_slug="latency-wall", version="3", file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertFalse(result.resolved)
        self.assertEqual(result.reason, "thread not found")
        self.assertIsNone(result.target_path)

    def test_unresolved_version_not_found(self) -> None:
        """Sibling thread exists but the cited version dir does not.

        Canary failure mode: a memo cites ``foo.3`` but ``foo`` only has
        ``foo.1`` and ``foo.2`` on disk (typo'd N).
        """
        # Sibling has v1 and v2 only; memo cites v3.
        self._make_sibling_thread("latency-wall", "1", "2")

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.3]]",
            other_slug="latency-wall", version="3", file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertFalse(result.resolved)
        self.assertEqual(result.reason, "version not found")

    def test_unresolved_latest_when_no_versions(self) -> None:
        """``.latest`` ref but no symlink AND no version dirs.

        The sibling thread directory exists (e.g., empty placeholder)
        but has no versions yet. The walk-to-highest fallback finds
        nothing → ``"latest unresolvable"``.
        """
        # Sibling dir exists with no version dirs.
        self._make_sibling_thread("latency-wall")  # no versions
        # Add some non-version dirs to make sure the regex doesn't pick
        # them up accidentally.
        (self.portfolio_dir / "latency-wall" / "drafts").mkdir()
        (self.portfolio_dir / "latency-wall" / "notes").mkdir()

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.latest]]",
            other_slug="latency-wall", version=LATEST, file=None,
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertFalse(result.resolved)
        self.assertEqual(result.reason, "latest unresolvable")

    def test_unresolved_file_not_found_inside_version_dir(self) -> None:
        """Version dir resolves cleanly but the cited file does not exist."""
        sibling = self._make_sibling_thread("latency-wall", "3")
        # Don't touch memo.md; the ref cites it but it's absent.
        del sibling

        ref = CrossThreadRef(
            line=1, raw="[[../latency-wall/latency-wall.3/memo.md]]",
            other_slug="latency-wall", version="3", file="memo.md",
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertFalse(result.resolved)
        self.assertEqual(result.reason, "file not found")

    def test_unresolved_exhibits_file_not_found(self) -> None:
        """Version dir exists but ``exhibits/<file>`` does not."""
        self._make_sibling_thread("technical-vision", "2")
        # Don't touch the exhibit.

        ref = CrossThreadRef(
            line=1,
            raw="[[../technical-vision/technical-vision.2/exhibits/figure-arch.png]]",
            other_slug="technical-vision", version="2",
            file="exhibits/figure-arch.png",
        )
        result = resolve_cross_thread_ref(ref, self.portfolio_dir)

        self.assertFalse(result.resolved)
        self.assertEqual(result.reason, "file not found")


# ---------------------------------------------------------------------------
# Batch helper — resolve_cross_thread_refs
# ---------------------------------------------------------------------------


class TestBatchResolver(_ResolverBase):
    """The convenience batch helper composes parse + resolve correctly."""

    def test_empty_text_returns_empty_list(self) -> None:
        """No refs in body → empty list (backwards-compat anchor).

        Critical AC for the issue: "threads with no cross-thread refs
        get byte-identical back-check behavior" — the batch helper
        returns an empty list and the reviewer's dim-3 sub-step short-
        circuits without affecting the score.
        """
        # Even a fully-populated portfolio produces no resolutions when
        # the memo body has no cross-thread refs.
        self._make_sibling_thread("latency-wall", "3")

        self.assertEqual(resolve_cross_thread_refs("", self.portfolio_dir), [])
        self.assertEqual(
            resolve_cross_thread_refs("Plain prose, no refs.", self.portfolio_dir),
            [],
        )

    def test_mixed_resolved_and_unresolved(self) -> None:
        """A memo with both resolved and unresolved refs returns mixed list."""
        # One sibling exists (resolves), one does not (thread not found).
        self._make_sibling_thread("latency-wall", "3")

        memo_text = (
            "Citing [[../latency-wall/latency-wall.3]] (resolves).\n"
            "Also [[../missing-thread/missing-thread.latest]] (does not).\n"
        )
        results = resolve_cross_thread_refs(memo_text, self.portfolio_dir)

        self.assertEqual(len(results), 2)
        self.assertTrue(results[0].resolved)
        self.assertFalse(results[1].resolved)
        self.assertEqual(results[1].reason, "thread not found")

    def test_results_preserve_source_line_order(self) -> None:
        """Results come back in source-line order."""
        # Three siblings, all resolve.
        self._make_sibling_thread("a", "1")
        self._make_sibling_thread("b", "2")
        self._make_sibling_thread("c", "3")

        memo_text = (
            "Line 1: [[../a/a.1]]\n"
            "Line 2: nothing here\n"
            "Line 3: [[../b/b.2]]\n"
            "Line 4: [[../c/c.3]]\n"
        )
        results = resolve_cross_thread_refs(memo_text, self.portfolio_dir)

        self.assertEqual(len(results), 3)
        self.assertEqual(results[0].ref.line, 1)
        self.assertEqual(results[0].ref.other_slug, "a")
        self.assertEqual(results[1].ref.line, 3)
        self.assertEqual(results[1].ref.other_slug, "b")
        self.assertEqual(results[2].ref.line, 4)
        self.assertEqual(results[2].ref.other_slug, "c")


# ---------------------------------------------------------------------------
# Integration tests — anchored to the on-disk fixture
# ---------------------------------------------------------------------------


class TestFixture(unittest.TestCase):
    """The cross_thread_refs fixture exists with the documented shape.

    The fixture pins the resolver's contract against a canary-shaped
    multi-thread portfolio so any future shape drift in the resolver or
    in the canary's on-disk layout surfaces here.
    """

    def test_fixture_dir_exists(self) -> None:
        self.assertTrue(
            _FIXTURE_DIR.is_dir(),
            msg=f"fixture dir not found at {_FIXTURE_DIR}",
        )

    def test_resolved_ref_to_sibling_memo_body(self) -> None:
        """A ref from one fixture thread to another's ``memo.md`` resolves.

        Demonstrates the AC: "reference to a sibling's ``memo.md`` body
        inside a version dir".
        """
        # Citing thread is ``alpha-memo`` (under the fixture). It cites
        # ``[[../beta-memo/beta-memo.latest/memo.md]]`` which resolves
        # via walk-to-highest (no symlink in this fixture path) and
        # beta-memo.2/memo.md exists.
        memo_text = (_FIXTURE_DIR / "alpha-memo" / "alpha-memo.1" / "memo.md").read_text(
            encoding="utf-8"
        )
        results = resolve_cross_thread_refs(memo_text, _FIXTURE_DIR)

        # The fixture's alpha-memo.1/memo.md has at least one resolved
        # cross-thread ref (to beta-memo.latest/memo.md).
        resolved = [r for r in results if r.resolved]
        self.assertTrue(
            len(resolved) >= 1,
            msg=f"expected at least one resolved cross-thread ref; got {results}",
        )

    def test_unresolved_typo_ref_in_fixture(self) -> None:
        """A typo'd N in a fixture ref surfaces as ``version not found``.

        Demonstrates the AC: "unresolved cross-thread ref (typo'd
        version number)".
        """
        memo_text = (_FIXTURE_DIR / "alpha-memo" / "alpha-memo.1" / "memo.md").read_text(
            encoding="utf-8"
        )
        results = resolve_cross_thread_refs(memo_text, _FIXTURE_DIR)

        # The fixture body includes a deliberately typo'd ref to
        # ``[[../beta-memo/beta-memo.99]]`` which should surface as
        # "version not found".
        unresolved_typo = [
            r for r in results
            if not r.resolved and r.reason == "version not found"
        ]
        self.assertTrue(
            len(unresolved_typo) >= 1,
            msg=(
                "expected at least one 'version not found' resolution "
                f"from fixture body; got {results}"
            ),
        )

    def test_unresolved_missing_thread_ref_in_fixture(self) -> None:
        """A ref to a non-existent thread surfaces as ``thread not found``.

        Demonstrates the AC: "unresolved cross-thread ref (missing
        slug)".
        """
        memo_text = (_FIXTURE_DIR / "alpha-memo" / "alpha-memo.1" / "memo.md").read_text(
            encoding="utf-8"
        )
        results = resolve_cross_thread_refs(memo_text, _FIXTURE_DIR)

        # The fixture body includes a deliberately missing-thread ref.
        unresolved_missing = [
            r for r in results
            if not r.resolved and r.reason == "thread not found"
        ]
        self.assertTrue(
            len(unresolved_missing) >= 1,
            msg=(
                "expected at least one 'thread not found' resolution "
                f"from fixture body; got {results}"
            ),
        )


# ---------------------------------------------------------------------------
# Module constants (sanity check)
# ---------------------------------------------------------------------------


class TestModuleConstants(unittest.TestCase):
    """Module constants surface the documented vocabulary."""

    def test_latest_constant_is_latest(self) -> None:
        """The ``LATEST`` constant is the literal string ``"latest"``.

        The constant is the single source of truth for the symbolic
        version specifier — coupled to sub-deliverable 5 (#288)'s
        ``.latest`` symlink contract. If that sub-deliverable renames
        the symbolic form, this constant is the one place to update.
        """
        self.assertEqual(LATEST, "latest")


# ---------------------------------------------------------------------------
# Dataclass surface (sanity check the public surface area)
# ---------------------------------------------------------------------------


class TestDataclassSurface(unittest.TestCase):
    """Sanity-check the public dataclasses are constructible and frozen."""

    def test_cross_thread_ref_constructible(self) -> None:
        ref = CrossThreadRef(
            line=1, raw="[[../x/x.1]]", other_slug="x", version="1", file=None
        )
        self.assertEqual(ref.line, 1)
        self.assertEqual(ref.other_slug, "x")

    def test_cross_thread_resolution_constructible(self) -> None:
        ref = CrossThreadRef(
            line=1, raw="[[../x/x.1]]", other_slug="x", version="1", file=None
        )
        resolution = CrossThreadResolution(
            ref=ref, target_path=None, resolved=False, reason="thread not found",
        )
        self.assertFalse(resolution.resolved)
        self.assertEqual(resolution.reason, "thread not found")


if __name__ == "__main__":
    unittest.main()
