"""Tests for ``anvil.skills.memo.lib.project_discovery`` (issues #284, #295).

Covers the project-as-thread-root discovery primitive. Originally shipped
as a dual-layout primitive (PR #290 for issue #284), simplified under
issue #295 to recognize only the project-brief layout — every memo
thread now lives inside a project root with a project-level
``BRIEF.md``.

Test coverage map (from issue #284 / #295 AC list):

- **Project-BRIEF layout** — a project with a BRIEF.md containing
  a non-empty ``documents:`` list returns ``LAYOUT_PROJECT_BRIEF`` for
  every listed slug.
- **Missing BRIEF** — a project-style directory shape without a
  BRIEF.md returns ``None`` (no longer a classic-layout fallback;
  every thread must be acknowledged by a project BRIEF).
- **BRIEF with empty `documents:` list** — does NOT trigger
  discovery; returns ``None``.
- **Unlisted slug** — a slug subdirectory inside a project that the
  BRIEF does NOT name returns ``None`` (conservative under #295: every
  thread root must be acknowledged by the project BRIEF).
- **Walk-upward from nested path** — discovery from a file inside a
  version dir resolves to the same thread root as discovery from the
  thread root itself.
- **No thread found** — a path that is neither under a thread nor
  inside a project returns ``None``.

Tests use ``tmp_path`` per test for the directory skeleton, plus an
on-disk fixture under ``fixtures/project_brief/`` that mirrors the
Studio canary's intended five-document project shape.

Per the #58 packaging convention, this file's filename
(``test_project_discovery.py``) is unique across the
``anvil/skills/*/tests/`` tree so the cross-skill pytest discovery
does not collide on basename.

Runs under either ``python -m unittest discover anvil/skills/memo/tests/``
or ``pytest anvil/skills/memo/tests/``.
"""

from __future__ import annotations

import sys
import textwrap
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


# The memo skill keeps its lib modules under its own ``lib/`` per the
# CLAUDE.md "skill-local first, lib promotion later" pattern. Add it to
# ``sys.path`` so tests import without a package install step — mirrors
# ``test_anvil_config.py`` and ``test_refs_resolver.py`` exactly.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))

from project_discovery import (  # noqa: E402
    BRIEF_FILENAME,
    DOCUMENTS_FRONTMATTER_KEY,
    DiscoveryResult,
    LAYOUT_PROJECT_BRIEF,
    discover_thread_root,
    has_project_brief,
)


_FIXTURES = _HERE / "fixtures" / "project_brief"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _write_project_brief(directory: Path, documents_yaml: str) -> Path:
    """Write a project BRIEF.md with the given ``documents:`` YAML body.

    ``documents_yaml`` is inserted under the ``documents:`` key in the
    frontmatter. Pass a YAML list literal (``"[]"`` for empty) or a
    multi-line block.
    """
    directory.mkdir(parents=True, exist_ok=True)
    brief = directory / BRIEF_FILENAME
    brief.write_text(
        textwrap.dedent(
            f"""\
            ---
            project: fixture-project
            audience: [test]
            documents: {documents_yaml}
            ---

            # Project BRIEF (fixture)
            """
        ),
        encoding="utf-8",
    )
    return brief


def _make_project_slug_dir(project: Path, slug: str, num_versions: int = 1) -> Path:
    """Create ``<project>/<slug>/`` with N version dirs and echo-named body files.

    Body filename echoes the slug per #295 (``<slug>.md``).
    """
    sd = project / slug
    sd.mkdir(parents=True, exist_ok=True)
    for i in range(1, num_versions + 1):
        v = sd / f"{slug}.{i}"
        v.mkdir(parents=True, exist_ok=True)
        (v / f"{slug}.md").write_text("# memo body\n", encoding="utf-8")
    return sd


class _TmpRootBase(unittest.TestCase):
    """Per-test temp dir; subclasses build the on-disk skeleton in ``setUp``."""

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.root = Path(self._td.name)
        self.addCleanup(self._td.cleanup)


# ---------------------------------------------------------------------------
# has_project_brief — the layout gate
# ---------------------------------------------------------------------------


class TestHasProjectBrief(_TmpRootBase):
    """``has_project_brief`` returns True only when documents: is non-empty list."""

    def test_no_brief_at_all(self) -> None:
        d = self.root / "empty"
        d.mkdir()
        self.assertFalse(has_project_brief(d))

    def test_brief_with_nonempty_documents(self) -> None:
        d = self.root / "project"
        _write_project_brief(d, "[{slug: memo-a, artifact_type: investment-memo}]")
        self.assertTrue(has_project_brief(d))

    def test_brief_with_empty_documents_list_returns_false(self) -> None:
        """A BRIEF with ``documents: []`` does NOT satisfy the layout gate."""
        d = self.root / "project"
        _write_project_brief(d, "[]")
        self.assertFalse(has_project_brief(d))

    def test_brief_with_documents_absent_returns_false(self) -> None:
        """A BRIEF with no ``documents:`` key at all returns False."""
        d = self.root / "thread"
        d.mkdir()
        (d / BRIEF_FILENAME).write_text(
            "---\ncompany: foo\n---\n\n# BRIEF\n",
            encoding="utf-8",
        )
        self.assertFalse(has_project_brief(d))

    def test_brief_with_documents_as_string_returns_false(self) -> None:
        """A non-list ``documents:`` value (string, dict, scalar) returns False."""
        d = self.root / "project"
        _write_project_brief(d, "memo-a")  # scalar, not list
        self.assertFalse(has_project_brief(d))

    def test_brief_with_no_frontmatter_returns_false(self) -> None:
        d = self.root / "thread"
        d.mkdir()
        (d / BRIEF_FILENAME).write_text("# Just a BRIEF with no frontmatter\n", encoding="utf-8")
        self.assertFalse(has_project_brief(d))

    def test_brief_with_malformed_yaml_returns_false(self) -> None:
        """Malformed YAML degrades to False (absence-tolerant)."""
        d = self.root / "thread"
        d.mkdir()
        (d / BRIEF_FILENAME).write_text(
            "---\ndocuments: [unclosed list\n---\n\n# BRIEF\n",
            encoding="utf-8",
        )
        self.assertFalse(has_project_brief(d))

    def test_brief_is_a_directory_not_file_returns_false(self) -> None:
        """Defensive: BRIEF.md as a directory (broken setup) returns False."""
        d = self.root / "thread"
        d.mkdir()
        (d / BRIEF_FILENAME).mkdir()
        self.assertFalse(has_project_brief(d))


# ---------------------------------------------------------------------------
# Project-BRIEF layout
# ---------------------------------------------------------------------------


class TestProjectBriefLayout(_TmpRootBase):
    """A project BRIEF with a non-empty documents: list triggers project-brief layout."""

    def _make_project(self) -> Path:
        project = self.root / "brains-for-robots"
        _write_project_brief(
            project,
            textwrap.dedent(
                """
                [
                  {slug: investment-memo, artifact_type: investment-memo},
                  {slug: latency-wall, artifact_type: position-paper},
                  {slug: technical-vision, artifact_type: vision-document}
                ]
                """
            ).strip(),
        )
        return project

    def test_project_brief_resolves_for_listed_slug_from_thread_root(self) -> None:
        project = self._make_project()
        thread_root = _make_project_slug_dir(project, "investment-memo")
        result = discover_thread_root(thread_root)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result.thread_root, thread_root)
        self.assertEqual(result.layout, LAYOUT_PROJECT_BRIEF)
        self.assertEqual(result.project_root, project)
        self.assertEqual(result.slug, "investment-memo")

    def test_project_brief_resolves_from_version_dir(self) -> None:
        project = self._make_project()
        thread_root = _make_project_slug_dir(project, "investment-memo", num_versions=2)
        version_dir = thread_root / "investment-memo.2"
        result = discover_thread_root(version_dir)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result.thread_root, thread_root)
        self.assertEqual(result.layout, LAYOUT_PROJECT_BRIEF)
        self.assertEqual(result.project_root, project)
        self.assertEqual(result.slug, "investment-memo")

    def test_project_brief_resolves_from_nested_file(self) -> None:
        project = self._make_project()
        thread_root = _make_project_slug_dir(project, "latency-wall")
        # Body filename echoes the slug per #295.
        memo_file = thread_root / "latency-wall.1" / "latency-wall.md"
        result = discover_thread_root(memo_file)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result.thread_root, thread_root)
        self.assertEqual(result.layout, LAYOUT_PROJECT_BRIEF)

    def test_project_brief_resolves_for_each_listed_slug(self) -> None:
        """All listed slugs resolve, each with its own thread_root."""
        project = self._make_project()
        slugs = ["investment-memo", "latency-wall", "technical-vision"]
        for slug in slugs:
            thread_root = _make_project_slug_dir(project, slug)
            result = discover_thread_root(thread_root)
            self.assertIsNotNone(result, f"slug {slug} should resolve")
            assert result is not None
            self.assertEqual(result.thread_root, thread_root)
            self.assertEqual(result.layout, LAYOUT_PROJECT_BRIEF)
            self.assertEqual(result.project_root, project)
            self.assertEqual(result.slug, slug)

    def test_project_brief_with_no_version_dirs_yet(self) -> None:
        """A slug dir without version dirs still resolves via project-brief lookup.

        The slug subdirectory exists but no draft has been written yet.
        Discovery from inside the slug dir walks up to the project root,
        recognizes the project BRIEF lists the slug, and returns the
        slug dir as the thread root.
        """
        project = self._make_project()
        slug_dir = project / "investment-memo"
        slug_dir.mkdir(parents=True)
        # Discovery from a hypothetical path inside the slug dir.
        # The slug_dir has no version dirs, so the version-dir check
        # at slug_dir fails. The walk continues to project, which is
        # a project root, and the path's first component relative to
        # project is "investment-memo" — the listed slug.
        hypothetical = slug_dir / "investment-memo.1" / "investment-memo.md"
        result = discover_thread_root(hypothetical)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result.thread_root, slug_dir)
        self.assertEqual(result.layout, LAYOUT_PROJECT_BRIEF)
        self.assertEqual(result.project_root, project)
        self.assertEqual(result.slug, "investment-memo")

    def test_unlisted_slug_inside_project_root_returns_none(self) -> None:
        """A subdirectory of the project that is NOT in the documents list.

        Conservative behavior: a stray subdirectory inside a project
        dir that the BRIEF doesn't name is treated as not-a-thread.
        Returns None rather than guessing.
        """
        project = self._make_project()
        # "stray-dir" is NOT in the project BRIEF's documents list.
        stray = project / "stray-dir"
        stray.mkdir(parents=True)
        result = discover_thread_root(stray)
        self.assertIsNone(result)

    def test_project_root_itself_returns_none(self) -> None:
        """Discovery from the project root with no further path component returns None."""
        project = self._make_project()
        result = discover_thread_root(project)
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Missing BRIEF
# ---------------------------------------------------------------------------


class TestMissingBrief(_TmpRootBase):
    """A subdirectory shape without a project BRIEF returns None.

    Under #295 every thread must live inside a project root. Without
    an acknowledging project BRIEF, discovery returns None — there is
    no classic-layout fallback.
    """

    def test_subdir_with_version_dirs_but_no_project_brief_returns_none(self) -> None:
        parent = self.root / "looks-like-project-but-isnt"
        slug = "demo-memo"
        thread = parent / slug
        thread.mkdir(parents=True)
        v = thread / f"{slug}.1"
        v.mkdir(parents=True)
        (v / f"{slug}.md").write_text("# memo body\n", encoding="utf-8")
        result = discover_thread_root(thread)
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Empty documents: list
# ---------------------------------------------------------------------------


class TestEmptyDocumentsList(_TmpRootBase):
    """A BRIEF with ``documents: []`` does NOT trigger discovery.

    Issue #284 AC carried over to #295: "BRIEF with empty
    ``documents:`` list" must NOT satisfy the layout gate. Under #295
    that means no thread under such a directory is discoverable
    (classic-layout fallback was retired).
    """

    def test_empty_documents_list_returns_none(self) -> None:
        parent = self.root / "project-shaped-but-empty"
        _write_project_brief(parent, "[]")
        # Build a slug-shaped subdir; without a non-empty documents list
        # discovery declines to resolve it.
        slug = "demo-memo"
        thread = parent / slug
        thread.mkdir(parents=True)
        v = thread / f"{slug}.1"
        v.mkdir(parents=True)
        (v / f"{slug}.md").write_text("# memo body\n", encoding="utf-8")
        result = discover_thread_root(thread)
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Slug not listed in project BRIEF
# ---------------------------------------------------------------------------


class TestSlugNotListed(_TmpRootBase):
    """A thread whose slug is NOT in the project BRIEF's documents list returns None.

    Under #295 the project-BRIEF acknowledgement is load-bearing: a
    stray slug subdir inside a project dir, not named in
    ``documents:``, is treated as not-a-thread.
    """

    def test_slug_not_in_documents_returns_none(self) -> None:
        project = self.root / "project"
        _write_project_brief(
            project, "[{slug: other-memo, artifact_type: investment-memo}]"
        )
        slug = "demo-memo"  # NOT in the documents list
        thread = project / slug
        thread.mkdir(parents=True)
        v = thread / f"{slug}.1"
        v.mkdir(parents=True)
        (v / f"{slug}.md").write_text("# memo body\n", encoding="utf-8")
        result = discover_thread_root(thread)
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# No thread found
# ---------------------------------------------------------------------------


class TestNoThreadFound(_TmpRootBase):
    """Paths that are neither under a thread nor inside a project return None."""

    def test_bare_path_returns_none(self) -> None:
        """A path that is just an ordinary directory with no anvil shape."""
        d = self.root / "just-a-dir"
        d.mkdir()
        result = discover_thread_root(d)
        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Fixture-based regression
# ---------------------------------------------------------------------------


class TestProjectBriefFixture(unittest.TestCase):
    """Regression: the on-disk fixture under ``fixtures/project_brief/`` resolves.

    The fixture mirrors the Studio canary's intended five-document
    project shape. It is the regression anchor for sub-deliverables
    2 (BRIEF parser, #285) and 3 (overlay selection, #286) when they
    wire the full schema parse and overlay dispatch.
    """

    def test_fixture_exists(self) -> None:
        """The fixture root should exist."""
        self.assertTrue(_FIXTURES.is_dir(), f"missing fixture root: {_FIXTURES}")

    def test_project_brief_fixture_resolves(self) -> None:
        """Each listed slug in the project BRIEF fixture resolves to project-brief."""
        project = _FIXTURES / "brains-for-robots"
        if not project.is_dir():
            self.skipTest(f"missing project fixture: {project}")
        # has_project_brief recognizes the project root.
        self.assertTrue(has_project_brief(project))

        # Each slug subdir resolves to LAYOUT_PROJECT_BRIEF.
        for slug in ("investment-memo", "latency-wall"):
            slug_dir = project / slug
            if not slug_dir.is_dir():
                continue
            result = discover_thread_root(slug_dir)
            self.assertIsNotNone(result, f"slug {slug} should resolve via fixture")
            assert result is not None
            self.assertEqual(result.layout, LAYOUT_PROJECT_BRIEF)
            self.assertEqual(result.project_root, project)
            self.assertEqual(result.slug, slug)
            self.assertEqual(result.thread_root, slug_dir)

    def test_empty_documents_fixture_returns_none_at_brief_gate(self) -> None:
        """A fixture with ``documents: []`` does not satisfy the layout gate."""
        empty = _FIXTURES / "empty-documents-project"
        if not empty.is_dir():
            self.skipTest(f"missing empty-documents fixture: {empty}")
        # has_project_brief recognizes this as NOT a project root.
        self.assertFalse(has_project_brief(empty))


# ---------------------------------------------------------------------------
# Constants are exported and stable
# ---------------------------------------------------------------------------


class TestConstants(unittest.TestCase):
    """The layout marker constant and on-disk literals are exported and stable."""

    def test_layout_constant_is_string(self) -> None:
        self.assertIsInstance(LAYOUT_PROJECT_BRIEF, str)
        self.assertEqual(LAYOUT_PROJECT_BRIEF, "project-brief")

    def test_brief_filename_constant(self) -> None:
        self.assertEqual(BRIEF_FILENAME, "BRIEF.md")

    def test_documents_key_constant(self) -> None:
        self.assertEqual(DOCUMENTS_FRONTMATTER_KEY, "documents")

    def test_layout_classic_is_removed(self) -> None:
        """``LAYOUT_CLASSIC`` is no longer exported (issue #295)."""
        import project_discovery as pd

        self.assertFalse(hasattr(pd, "LAYOUT_CLASSIC"))

    def test_discovery_result_is_frozen(self) -> None:
        """DiscoveryResult should be immutable (frozen dataclass)."""
        r = DiscoveryResult(
            thread_root=Path("/tmp/x"),
            layout=LAYOUT_PROJECT_BRIEF,
            project_root=Path("/tmp"),
            slug="x",
        )
        with self.assertRaises(Exception):
            r.layout = "other"  # type: ignore[misc]


if __name__ == "__main__":
    unittest.main()
