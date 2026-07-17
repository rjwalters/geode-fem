"""Tests for the memo asset resolver (issue #322 — Phase A).

Covers ``anvil.skills.memo.lib.theme_resolver.resolve_memo_asset`` plus
the integration point with ``ProjectBrief.theme`` (issue #322 schema
add).

Precedence under test::

    <consumer>/.anvil/themes/<theme>/memo/<asset>
        >  framework default at anvil/lib/memo/<asset>

The middle "consumer single-tenant override" tier (the ``memo/<asset>``
subtree of the legacy consumer ``.anvil/lib/`` directory) is no longer
load-bearing post-#230 — the install layout puts anvil itself under
``<consumer>/.anvil/anvil/lib/memo/`` and the framework-default lookup
resolves there automatically. So Phase A only tests two tiers: per-
theme override + framework default. (When the canary adopts the
post-#230 layout, single-tenant override is just in-place editing of
the framework default file.)

Per the #58 packaging convention, this file's filename
(``test_theme_resolution.py``) is unique across the
``anvil/skills/*/tests/`` tree.

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
# ``test_project_brief.py`` and ``test_project_discovery.py`` exactly.
_HERE = Path(__file__).resolve().parent
_LIB = _HERE.parent / "lib"
sys.path.insert(0, str(_LIB))


from theme_resolver import (  # noqa: E402
    MEMO_ASSET_NAMES,
    MEMO_ASSET_STYLES_CSS,
    MEMO_ASSET_TEMPLATE_HTML,
    MEMO_ASSET_TEMPLATE_TEX,
    resolve_memo_asset,
)
from project_brief import (  # noqa: E402
    BriefDocument,
    ProjectBrief,
    load_project_brief,
)
from project_discovery import BRIEF_FILENAME  # noqa: E402


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _seed_consumer(root: Path) -> None:
    """Create the ``.anvil/`` marker so find_consumer_root succeeds."""
    (root / ".anvil").mkdir(parents=True, exist_ok=True)


def _write_theme_asset(
    consumer: Path, theme: str, asset_name: str, body: str
) -> Path:
    """Write a per-theme override asset and return its path."""
    target_dir = consumer / ".anvil" / "themes" / theme / "memo"
    target_dir.mkdir(parents=True, exist_ok=True)
    target = target_dir / asset_name
    target.write_text(body, encoding="utf-8")
    return target


# ---------------------------------------------------------------------------
# resolve_memo_asset — closed-set API
# ---------------------------------------------------------------------------


class TestResolverAcceptedAssetNames(unittest.TestCase):
    """The resolver is a closed-set API; unknown names raise."""

    def test_known_assets_resolve(self):
        for name in MEMO_ASSET_NAMES:
            path = resolve_memo_asset(
                name, consumer_root=None, theme_name=None
            )
            # Framework default location is always returned for None/None.
            self.assertTrue(
                path.name == name,
                f"resolver returned {path} for asset {name!r}",
            )

    def test_unknown_asset_raises(self):
        with self.assertRaises(ValueError) as ctx:
            resolve_memo_asset(
                "memo.json", consumer_root=None, theme_name=None
            )
        msg = str(ctx.exception)
        # The error names the offending value and the accepted set.
        self.assertIn("memo.json", msg)
        self.assertIn("template.html", msg)


# ---------------------------------------------------------------------------
# resolve_memo_asset — framework default tier
# ---------------------------------------------------------------------------


class TestResolverFrameworkDefault(unittest.TestCase):
    """When no theme tier applies, the framework default wins."""

    def test_no_consumer_root_returns_framework_default(self):
        path = resolve_memo_asset(
            MEMO_ASSET_STYLES_CSS, consumer_root=None, theme_name=None
        )
        # The shipped default file is guaranteed to exist.
        self.assertTrue(path.is_file(), f"missing framework default: {path}")
        self.assertEqual(path.name, "styles.css")
        # Path is under anvil/lib/memo/.
        self.assertEqual(path.parent.name, "memo")

    def test_no_theme_name_returns_framework_default(self):
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            path = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_TEX,
                consumer_root=consumer,
                theme_name=None,
            )
            self.assertTrue(path.is_file())
            self.assertEqual(path.parent.name, "memo")

    def test_empty_theme_name_returns_framework_default(self):
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            path = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_HTML,
                consumer_root=consumer,
                theme_name="   ",
            )
            self.assertTrue(path.is_file())
            self.assertEqual(path.parent.name, "memo")

    def test_missing_theme_dir_returns_framework_default(self):
        """Theme name pointing to a missing dir → fallthrough, no raise."""
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            # No <consumer>/.anvil/themes/ghost/memo/ written.
            path = resolve_memo_asset(
                MEMO_ASSET_STYLES_CSS,
                consumer_root=consumer,
                theme_name="ghost",
            )
            self.assertTrue(path.is_file())
            self.assertEqual(path.parent.name, "memo")

    def test_missing_specific_asset_returns_framework_default(self):
        """Theme dir present, but the specific asset absent → fallthrough."""
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            # Write only styles.css under the theme; ask for template.tex.
            _write_theme_asset(
                consumer, "sphere-semi", MEMO_ASSET_STYLES_CSS, "body{}"
            )
            tex_path = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_TEX,
                consumer_root=consumer,
                theme_name="sphere-semi",
            )
            self.assertTrue(tex_path.is_file())
            self.assertEqual(tex_path.parent.name, "memo")


# ---------------------------------------------------------------------------
# resolve_memo_asset — per-theme tier wins
# ---------------------------------------------------------------------------


class TestResolverThemeTier(unittest.TestCase):
    """When the per-theme asset exists, it wins over the framework default."""

    def test_styles_css_theme_override(self):
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            written = _write_theme_asset(
                consumer,
                "sphere-semi",
                MEMO_ASSET_STYLES_CSS,
                "/* Sphere Semi brand styles */\n@page { margin: 1in; }\n",
            )
            resolved = resolve_memo_asset(
                MEMO_ASSET_STYLES_CSS,
                consumer_root=consumer,
                theme_name="sphere-semi",
            )
            self.assertEqual(resolved, written)
            # The resolved path's content matches what we wrote.
            self.assertIn("Sphere Semi", resolved.read_text())

    def test_template_html_theme_override(self):
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            written = _write_theme_asset(
                consumer,
                "2am-logic",
                MEMO_ASSET_TEMPLATE_HTML,
                "<!-- 2AM Logic memo template -->\n",
            )
            resolved = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_HTML,
                consumer_root=consumer,
                theme_name="2am-logic",
            )
            self.assertEqual(resolved, written)

    def test_template_tex_theme_override(self):
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            written = _write_theme_asset(
                consumer,
                "sphere-semi",
                MEMO_ASSET_TEMPLATE_TEX,
                "% Sphere Semi xelatex template\n",
            )
            resolved = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_TEX,
                consumer_root=consumer,
                theme_name="sphere-semi",
            )
            self.assertEqual(resolved, written)

    def test_theme_override_for_one_asset_does_not_affect_others(self):
        """A theme overriding only styles.css still uses the default
        template.tex / template.html."""
        with TemporaryDirectory() as td:
            consumer = Path(td)
            _seed_consumer(consumer)
            theme_css = _write_theme_asset(
                consumer,
                "partial",
                MEMO_ASSET_STYLES_CSS,
                "body { font-family: serif; }",
            )
            # styles.css comes from the theme tier.
            css_path = resolve_memo_asset(
                MEMO_ASSET_STYLES_CSS,
                consumer_root=consumer,
                theme_name="partial",
            )
            self.assertEqual(css_path, theme_css)
            # template.html and template.tex come from the framework default.
            html_path = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_HTML,
                consumer_root=consumer,
                theme_name="partial",
            )
            tex_path = resolve_memo_asset(
                MEMO_ASSET_TEMPLATE_TEX,
                consumer_root=consumer,
                theme_name="partial",
            )
            # Both land in the framework default location, NOT the theme dir.
            self.assertEqual(html_path.parent.name, "memo")
            self.assertNotIn("themes", str(html_path))
            self.assertEqual(tex_path.parent.name, "memo")
            self.assertNotIn("themes", str(tex_path))


# ---------------------------------------------------------------------------
# ProjectBrief.theme field — the schema extension that drives the resolver
# ---------------------------------------------------------------------------


def _write_brief(
    directory: Path, frontmatter: str, body: str = "\n# Project BRIEF\n"
) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    brief = directory / BRIEF_FILENAME
    brief.write_text(
        f"---\n{frontmatter}\n---\n{body}", encoding="utf-8"
    )
    return brief


class TestProjectBriefThemeField(unittest.TestCase):
    """Issue #322: ``theme:`` is a new optional key in BRIEF.md frontmatter."""

    def test_brief_without_theme_field_loads_with_none(self):
        with TemporaryDirectory() as td:
            project = Path(td) / "demo"
            _write_brief(
                project,
                textwrap.dedent(
                    """\
                    project: demo
                    documents:
                      - slug: memo
                        artifact_type: investment-memo
                    """
                ).rstrip(),
            )
            brief = load_project_brief(project)
            self.assertIsNotNone(brief)
            self.assertIsNone(brief.theme)

    def test_brief_with_theme_field_loads_correctly(self):
        with TemporaryDirectory() as td:
            project = Path(td) / "demo"
            _write_brief(
                project,
                textwrap.dedent(
                    """\
                    project: demo
                    theme: sphere-semi
                    documents:
                      - slug: memo
                        artifact_type: investment-memo
                    """
                ).rstrip(),
            )
            brief = load_project_brief(project)
            self.assertIsNotNone(brief)
            self.assertEqual(brief.theme, "sphere-semi")

    def test_brief_with_empty_theme_field_is_normalized_to_none(self):
        with TemporaryDirectory() as td:
            project = Path(td) / "demo"
            _write_brief(
                project,
                textwrap.dedent(
                    """\
                    project: demo
                    theme: "   "
                    documents:
                      - slug: memo
                        artifact_type: investment-memo
                    """
                ).rstrip(),
            )
            brief = load_project_brief(project)
            self.assertIsNotNone(brief)
            self.assertIsNone(brief.theme)

    def test_brief_with_non_string_theme_raises(self):
        with TemporaryDirectory() as td:
            project = Path(td) / "demo"
            _write_brief(
                project,
                textwrap.dedent(
                    """\
                    project: demo
                    theme: [list, of, names]
                    documents:
                      - slug: memo
                        artifact_type: investment-memo
                    """
                ).rstrip(),
            )
            with self.assertRaises(ValueError) as ctx:
                load_project_brief(project)
            self.assertIn("theme", str(ctx.exception))

    def test_brief_with_null_theme_field_loads_with_none(self):
        with TemporaryDirectory() as td:
            project = Path(td) / "demo"
            _write_brief(
                project,
                textwrap.dedent(
                    """\
                    project: demo
                    theme: null
                    documents:
                      - slug: memo
                        artifact_type: investment-memo
                    """
                ).rstrip(),
            )
            brief = load_project_brief(project)
            self.assertIsNotNone(brief)
            self.assertIsNone(brief.theme)


# ---------------------------------------------------------------------------
# ProjectBrief — Pydantic field-level validation (programmatic construction)
# ---------------------------------------------------------------------------


class TestProjectBriefThemeProgrammatic(unittest.TestCase):
    """Build ProjectBrief directly to exercise the typed field."""

    def _doc(self) -> BriefDocument:
        return BriefDocument(slug="memo", artifact_type="investment-memo")

    def test_theme_field_optional(self):
        brief = ProjectBrief(
            project="demo",
            documents=[self._doc()],
        )
        self.assertIsNone(brief.theme)

    def test_theme_field_accepts_string(self):
        brief = ProjectBrief(
            project="demo",
            documents=[self._doc()],
            theme="sphere-semi",
        )
        self.assertEqual(brief.theme, "sphere-semi")


if __name__ == "__main__":
    unittest.main()
