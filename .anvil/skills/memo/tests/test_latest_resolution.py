"""Tests for ``anvil.skills.memo.lib.latest_resolution`` (issue #288).

Covers the canonical ``.latest`` resolver shipped under issue #288
(sub-deliverable 5 of #283): the single-source-of-truth helper
``resolve_latest(thread_dir, slug)`` that codifies the four-step
resolution rule the cross-thread reference resolver (#287) uses today
and that any future intra-thread or downstream caller can reuse:

1. ``<thread_dir>/<slug>.latest`` exists as a **symlink** → return it
   (an author can pin ``.latest`` to a non-highest version
   intentionally).
2. Else if ``<thread_dir>/<slug>.latest`` exists as a **real directory**
   → return it.
3. Else, walk ``<thread_dir>`` for ``<slug>.<N>/`` children and return
   the highest-numbered one.
4. Else, return ``None``.

Acceptance criteria from issue #288 (option (c) — the curator-
recommended pure-tolerance path):

- The reviewer back-check tolerates ``<slug>.latest`` references by
  resolving to the highest-numbered ``<slug>.<N>/`` directory when no
  ``.latest`` symlink exists. → ``TestWalkToHighest``.
- If ``<slug>.latest`` symlink exists, it takes precedence (an author
  can pin ``.latest`` to a non-highest version intentionally). →
  ``TestSymlinkWins`` (including the load-bearing
  ``test_pinned_symlink_to_non_highest_version_honored``).
- Documented in ``SKILL.md`` and ``rubric.md`` as the canonical
  resolution rule. → ``TestDocCanonicalReferences``.
- Unit tests cover: real symlink resolves correctly; no symlink,
  multiple ``<slug>.N`` dirs → highest N wins; no symlink, no version
  dirs → ``None`` (clear error at caller layer); pinned ``.latest``
  pointing at non-highest version is honored. → covered across all
  classes below.

Per the #58 packaging convention, this file's filename
(``test_latest_resolution.py``) is unique across the
``anvil/skills/*/tests/`` tree so cross-skill pytest discovery does not
collide on basename. The companion fixture lives at
``tests/fixtures/latest_symlink/`` (verified distinct from the existing
``cross_thread_refs/``, ``project_brief/``, ``project_brief_parser/``,
``portfolio_shared_refs/``, ``cross_thread_cite_consistency/``,
``summary_detail_consistency/``, and ``rubric_overrides/`` fixture
directories by direct inspection).

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


# Mirror the test_cross_thread_refs.py / test_refs_resolver.py sys.path
# injection — the memo skill keeps its lib modules under its own
# ``lib/`` per CLAUDE.md "skill-local first, lib promotion later" and
# we want to import without packaging install gymnastics.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from anvil.skills.memo.lib.latest_resolution import (  # noqa: E402
    LATEST,
    resolve_latest,
)


_FIXTURE_DIR = _HERE / "fixtures" / "latest_symlink"


# ---------------------------------------------------------------------------
# Test base: per-test temp dir for the thread skeleton
# ---------------------------------------------------------------------------


class _ResolverBase(unittest.TestCase):
    """Per-test temp dir for thread-with-versions skeleton.

    Provides ``self.thread_dir`` (the dir that contains version dirs)
    and helper methods to populate it with version directories, real
    ``.latest`` directories, and ``.latest`` symlinks.

    The temp dir is cleaned up automatically via
    ``self.addCleanup(self._td.cleanup)`` — no leaked test fixtures.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.thread_dir = Path(self._td.name) / "my-thread"
        self.thread_dir.mkdir(parents=True, exist_ok=True)
        self.slug = "my-thread"
        self.addCleanup(self._td.cleanup)

    def _make_version(self, n: int) -> Path:
        """Create ``<thread_dir>/<slug>.<N>/`` and return the path."""
        v = self.thread_dir / f"{self.slug}.{n}"
        v.mkdir(parents=True, exist_ok=True)
        return v

    def _make_real_latest(self) -> Path:
        """Create a REAL ``<slug>.latest/`` directory (not a symlink)."""
        latest = self.thread_dir / f"{self.slug}.{LATEST}"
        latest.mkdir(parents=True, exist_ok=True)
        return latest

    def _make_latest_symlink(self, target_n: int) -> Path:
        """Create ``<slug>.latest`` symlink pointing at ``<slug>.<N>/``.

        Uses a relative symlink so the fixture is filesystem-location-
        independent — matches the test_cross_thread_refs convention.
        """
        target = self.thread_dir / f"{self.slug}.{target_n}"
        # Caller is responsible for having created the target dir; we
        # don't auto-create here so tests can exercise dangling-symlink
        # behavior intentionally.
        symlink = self.thread_dir / f"{self.slug}.{LATEST}"
        os.symlink(target.name, symlink)
        return symlink


# ---------------------------------------------------------------------------
# Symlink precedence — the load-bearing AC
# ---------------------------------------------------------------------------


class TestSymlinkWins(_ResolverBase):
    """Step 1 of the four-step rule: symlink at ``.latest`` wins."""

    def test_symlink_pointing_at_highest_resolves_to_symlink_path(self) -> None:
        """Symlink points at the highest version dir → return symlink path.

        Confirms the resolver returns the **symlink path itself**, not
        the dereferenced target. This matches the pre-#288 cross-thread
        resolver's behavior — the operator-visible ``.latest`` path is
        what gets recorded in downstream tooling (``comments.md``,
        ``_summary.md``).
        """
        self._make_version(1)
        self._make_version(2)
        self._make_version(3)
        self._make_latest_symlink(target_n=3)

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.{LATEST}")

    def test_pinned_symlink_to_non_highest_version_honored(self) -> None:
        """Symlink pinned to non-highest version is honored.

        **Load-bearing AC from issue #288**: "If ``<slug>.latest``
        symlink exists, it takes precedence (an author can pin
        ``.latest`` to a non-highest version intentionally)."

        On disk: v1, v2, v3 exist but ``.latest -> v2``. The resolver
        MUST return the symlink (pointing at v2), not walk to v3.
        """
        self._make_version(1)
        self._make_version(2)
        self._make_version(3)
        # Intentional pin to v2 even though v3 exists.
        self._make_latest_symlink(target_n=2)

        result = resolve_latest(self.thread_dir, self.slug)

        # The returned path is the symlink itself; resolving it would
        # take the caller to <thread_dir>/<slug>.2. That dereference is
        # the caller's responsibility — we surface the operator-visible
        # path.
        self.assertEqual(result, self.thread_dir / f"{self.slug}.{LATEST}")
        # Confirm the symlink does target v2 (sanity check on the
        # fixture, not the resolver — but documents the contract).
        self.assertTrue(result.is_symlink())
        self.assertEqual(os.readlink(result), f"{self.slug}.2")

    def test_dangling_symlink_still_returned(self) -> None:
        """A dangling ``.latest`` symlink is returned (operator created it).

        The operator intentionally created the symlink; returning it is
        the right thing even if the target has since been deleted. The
        caller surfaces the consequence (e.g., the cross-thread resolver
        records ``"file not found"`` on a child file lookup).
        """
        # No version dirs at all. Symlink points at a nonexistent target.
        symlink = self.thread_dir / f"{self.slug}.{LATEST}"
        os.symlink(f"{self.slug}.99", symlink)

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, symlink)
        self.assertTrue(result.is_symlink())

    def test_symlink_precedence_over_real_latest_directory_does_not_arise(self) -> None:
        """A ``.latest`` cannot be BOTH a symlink and a real dir.

        Documents the precedence chain edge case: filesystems disallow
        a dirent from being two shapes at once, so step 1 and step 2
        of the resolver are mutually exclusive at the on-disk level. We
        document this by not even exercising the impossible case — the
        precedence is well-defined.
        """
        # No assertion needed; the test exists as documentation of the
        # precedence rule's structural constraint.
        pass


# ---------------------------------------------------------------------------
# Real .latest/ directory (step 2)
# ---------------------------------------------------------------------------


class TestRealLatestDirectory(_ResolverBase):
    """Step 2: real ``.latest/`` directory (no symlink) is returned."""

    def test_real_latest_directory_resolves(self) -> None:
        """``<slug>.latest/`` is a real directory → return it.

        The rarer shape — typically the operator hasn't migrated to the
        symlink convention yet, or is on a platform where symlinks are
        awkward (Windows without WSL).
        """
        self._make_real_latest()

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.{LATEST}")
        self.assertTrue(result.is_dir())
        self.assertFalse(result.is_symlink())

    def test_real_latest_directory_wins_over_walk_to_highest(self) -> None:
        """Real ``.latest/`` dir wins over walk-to-highest.

        Even when ``<slug>.5/`` exists on disk alongside a real
        ``<slug>.latest/`` dir, the resolver returns the
        ``<slug>.latest/`` path — step 2 fires before step 3.
        """
        self._make_version(5)
        self._make_real_latest()

        result = resolve_latest(self.thread_dir, self.slug)

        # Real .latest/ wins; the v5 dir is not returned.
        self.assertEqual(result, self.thread_dir / f"{self.slug}.{LATEST}")


# ---------------------------------------------------------------------------
# Walk-to-highest fallback (step 3) — the load-bearing AC
# ---------------------------------------------------------------------------


class TestWalkToHighest(_ResolverBase):
    """Step 3: no ``.latest`` of any shape → walk children, pick highest."""

    def test_single_version_resolves_to_that_version(self) -> None:
        """Only ``<slug>.1/`` exists → resolve to it."""
        self._make_version(1)

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.1")

    def test_multiple_versions_resolve_to_highest(self) -> None:
        """``<slug>.1/``, ``<slug>.2/``, ``<slug>.3/`` exist → resolve to .3.

        **Load-bearing AC from issue #288**: "The reviewer back-check
        tolerates ``<slug>.latest`` references by resolving to the
        highest-numbered ``<slug>.<N>/`` directory when no ``.latest``
        symlink exists."
        """
        self._make_version(1)
        self._make_version(2)
        self._make_version(3)

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.3")

    def test_non_contiguous_versions_resolve_to_highest(self) -> None:
        """Gaps in N (v1, v3, v7) → still resolve to the highest (v7).

        Real-world threads sometimes skip versions (a polish pass that
        produces v5 from v3 directly, a manual numbering scheme). The
        resolver picks the maximum N seen, not the count of dirs.
        """
        self._make_version(1)
        self._make_version(3)
        self._make_version(7)

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.7")

    def test_double_digit_versions_resolve_numerically(self) -> None:
        """``<slug>.2/`` vs ``<slug>.10/`` → .10 wins (numeric, not lex).

        The walk-to-highest fallback parses N as an integer, not a
        string. A lex-sort would pick v2 over v10 ("2" > "10" in ASCII),
        breaking long-arc threads. This test pins the numeric ordering.
        """
        self._make_version(2)
        self._make_version(10)
        self._make_version(11)

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.11")

    def test_unrelated_dirs_are_ignored(self) -> None:
        """Sibling dirs that don't match ``<slug>.<N>/`` are skipped.

        Confirms ``refs/``, ``BRIEF.md``, ``.review/`` siblings, etc.
        do not pollute the resolution. Only ``<slug>.<N>/`` children
        are eligible.
        """
        self._make_version(1)
        self._make_version(2)
        # Non-matching siblings.
        (self.thread_dir / "refs").mkdir()
        (self.thread_dir / f"{self.slug}.1.review").mkdir()
        (self.thread_dir / "drafts").mkdir()
        (self.thread_dir / "BRIEF.md").write_text("# brief\n", encoding="utf-8")
        # A version-shaped dir for a DIFFERENT slug (regex must reject).
        (self.thread_dir / "other-slug.99").mkdir()

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertEqual(result, self.thread_dir / f"{self.slug}.2")

    def test_critic_sibling_dirs_are_ignored(self) -> None:
        """``<slug>.<N>.<critic>/`` siblings do NOT match the version regex.

        The pattern ``<slug>.1.review`` has the slug + dot + N + dot +
        tag shape that anvil uses for critic siblings (see
        ``anvil/lib/snippets/thread_state.md``). The resolver's regex
        anchors on ``$`` after ``\\d+`` so these don't match.
        """
        self._make_version(1)
        # A critic sibling at .2 but NO version dir at .2.
        (self.thread_dir / f"{self.slug}.2.review").mkdir()
        (self.thread_dir / f"{self.slug}.2.audit").mkdir()

        result = resolve_latest(self.thread_dir, self.slug)

        # The critic siblings do NOT count as versions — the highest
        # real version is v1.
        self.assertEqual(result, self.thread_dir / f"{self.slug}.1")


# ---------------------------------------------------------------------------
# No resolution possible (step 4) — clean None return
# ---------------------------------------------------------------------------


class TestNoResolution(_ResolverBase):
    """Step 4: no version dirs and no ``.latest`` → ``None``.

    AC from issue #288: "no symlink, no version dirs → clear error".
    The resolver returns ``None``; the caller surfaces the error at its
    preferred granularity (the cross-thread resolver records
    ``"latest unresolvable"``).
    """

    def test_empty_thread_dir_returns_none(self) -> None:
        """Empty thread directory → ``None``."""
        result = resolve_latest(self.thread_dir, self.slug)

        self.assertIsNone(result)

    def test_thread_dir_with_only_non_version_dirs_returns_none(self) -> None:
        """Thread dir with only non-version children → ``None``."""
        (self.thread_dir / "refs").mkdir()
        (self.thread_dir / "drafts").mkdir()
        (self.thread_dir / "BRIEF.md").write_text("# brief\n", encoding="utf-8")

        result = resolve_latest(self.thread_dir, self.slug)

        self.assertIsNone(result)

    def test_nonexistent_thread_dir_returns_none(self) -> None:
        """``thread_dir`` itself doesn't exist → ``None`` (non-throwing).

        The lenient-form precedent: filesystem errors degrade to
        ``None``, not exceptions. The caller surfaces the error.
        """
        nonexistent = self.thread_dir / "does-not-exist"

        result = resolve_latest(nonexistent, "any-slug")

        self.assertIsNone(result)

    def test_thread_dir_is_a_file_returns_none(self) -> None:
        """``thread_dir`` exists but is a file → ``None``.

        Defensive: a misconfigured caller might pass a file path. The
        resolver returns ``None`` rather than crashing.
        """
        a_file = self.thread_dir / "not-a-dir"
        a_file.write_text("oops", encoding="utf-8")

        result = resolve_latest(a_file, "any-slug")

        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Slug shape variations
# ---------------------------------------------------------------------------


class TestSlugShape(_ResolverBase):
    """The slug regex correctly escapes special chars and matches variants."""

    def test_slug_with_hyphen_matches(self) -> None:
        """Hyphenated slug (``brasidas-synthesis``) resolves correctly.

        The canary's slugs are hyphenated (``investment-memo``,
        ``brasidas-synthesis``, ``latency-wall``). The resolver MUST
        escape the slug in the regex so the hyphen is literal.
        """
        slug = "brasidas-synthesis"
        thread = Path(self._td.name) / slug
        thread.mkdir(parents=True, exist_ok=True)
        (thread / f"{slug}.1").mkdir()
        (thread / f"{slug}.2").mkdir()

        result = resolve_latest(thread, slug)

        self.assertEqual(result, thread / f"{slug}.2")

    def test_slug_with_underscore_matches(self) -> None:
        """Underscored slug resolves correctly."""
        slug = "my_thread"
        thread = Path(self._td.name) / slug
        thread.mkdir(parents=True, exist_ok=True)
        (thread / f"{slug}.5").mkdir()

        result = resolve_latest(thread, slug)

        self.assertEqual(result, thread / f"{slug}.5")

    def test_slug_with_dots_still_works_via_escape(self) -> None:
        """A slug containing a dot is escaped (regex doesn't over-match).

        Defensive: a slug like ``v1.0-launch`` would, without
        ``re.escape``, let the dot match any character. The version
        regex escapes the slug so ``v1.0-launch.1/`` matches but
        ``v1X0-launch.1/`` does not.
        """
        slug = "v1.0-launch"
        thread = Path(self._td.name) / slug
        thread.mkdir(parents=True, exist_ok=True)
        (thread / f"{slug}.3").mkdir()
        # A directory that WOULD match if the slug's dot weren't
        # escaped — but it shouldn't.
        (thread / "v1X0-launch.99").mkdir()

        result = resolve_latest(thread, slug)

        self.assertEqual(result, thread / f"{slug}.3")


# ---------------------------------------------------------------------------
# Fixture integration test
# ---------------------------------------------------------------------------


class TestFixture(unittest.TestCase):
    """The ``latest_symlink`` fixture exists with the documented shape.

    The fixture pins the resolver's contract against an on-disk
    skeleton so any future shape drift in the resolver surfaces here.
    """

    def test_fixture_dir_exists(self) -> None:
        self.assertTrue(
            _FIXTURE_DIR.is_dir(),
            msg=f"fixture dir not found at {_FIXTURE_DIR}",
        )

    def test_fixture_walk_to_highest_resolves(self) -> None:
        """The ``walk-to-highest`` fixture thread resolves to its max N.

        Fixture shape:

            <FIXTURE>/walk-to-highest/
              walk-to-highest.1/
              walk-to-highest.2/
              walk-to-highest.3/
            (no .latest of any shape)
        """
        thread = _FIXTURE_DIR / "walk-to-highest"
        self.assertTrue(thread.is_dir(), msg=f"fixture thread missing: {thread}")

        result = resolve_latest(thread, "walk-to-highest")

        self.assertEqual(result, thread / "walk-to-highest.3")

    def test_fixture_pinned_symlink_resolves(self) -> None:
        """The ``pinned-symlink`` fixture honors the operator's pin.

        Fixture shape:

            <FIXTURE>/pinned-symlink/
              pinned-symlink.1/
              pinned-symlink.2/
              pinned-symlink.3/
              pinned-symlink.latest -> pinned-symlink.2  (pinned)
        """
        thread = _FIXTURE_DIR / "pinned-symlink"
        self.assertTrue(thread.is_dir(), msg=f"fixture thread missing: {thread}")

        result = resolve_latest(thread, "pinned-symlink")

        # The symlink path itself is what we return.
        self.assertEqual(result, thread / "pinned-symlink.latest")
        # And it's actually a symlink to .2 per the fixture contract.
        self.assertTrue(result.is_symlink())


# ---------------------------------------------------------------------------
# Documentation back-check
# ---------------------------------------------------------------------------


class TestDocCanonicalReferences(unittest.TestCase):
    """SKILL.md and rubric.md cite the canonical ``resolve_latest`` helper.

    AC from issue #288: "Documented in SKILL.md and rubric.md as the
    canonical resolution rule." This test pins the cross-link so any
    future doc drift surfaces here.
    """

    def _read_doc(self, relpath: str) -> str:
        # Walk up from this test file to the memo skill root, then read
        # the relative path.
        memo_root = _HERE.parent
        return (memo_root / relpath).read_text(encoding="utf-8")

    def test_skill_md_cites_latest_resolution(self) -> None:
        """``SKILL.md`` documents the canonical resolver path."""
        text = self._read_doc("SKILL.md")
        self.assertIn("latest_resolution", text)
        self.assertIn("resolve_latest", text)

    def test_rubric_md_cites_latest_resolution(self) -> None:
        """``rubric.md`` references the canonical resolver in dim 3.

        The dim-3 back-check uses the resolver to honor ``.latest`` refs;
        the rubric prose documents the contract so a reviewer reading
        the rubric sees the resolution rule without chasing into the
        Python module.
        """
        text = self._read_doc("rubric.md")
        self.assertIn("latest_resolution", text)


# ---------------------------------------------------------------------------
# Module constant sanity
# ---------------------------------------------------------------------------


class TestModuleConstants(unittest.TestCase):
    """Module constants pin the documented vocabulary."""

    def test_latest_constant_is_latest(self) -> None:
        """``LATEST`` is the literal string ``"latest"``.

        Coupled to the consumer-side ``.latest`` symlink convention
        documented in ``anvil/lib/snippets/version_layout.md`` and to
        ``cross_thread_refs.LATEST`` (re-exported from this module so
        callers have one source of truth).
        """
        self.assertEqual(LATEST, "latest")

    def test_cross_thread_refs_re_exports_same_latest(self) -> None:
        """``cross_thread_refs.LATEST`` is the same constant as ours.

        Confirms the re-export wiring works — a caller that already
        imports ``LATEST`` from ``cross_thread_refs`` is not affected
        by the move.
        """
        from cross_thread_refs import LATEST as CTR_LATEST  # noqa: E402

        self.assertEqual(LATEST, CTR_LATEST)

    def test_cross_thread_refs_re_exports_resolve_latest(self) -> None:
        """``cross_thread_refs.resolve_latest`` is the same callable.

        Confirms the re-export of the helper from
        ``cross_thread_refs.__all__`` — a downstream tooling caller
        that prefers the cross-thread import path can use it.
        """
        from cross_thread_refs import resolve_latest as CTR_resolve_latest  # noqa: E402

        self.assertIs(resolve_latest, CTR_resolve_latest)


if __name__ == "__main__":
    unittest.main()
