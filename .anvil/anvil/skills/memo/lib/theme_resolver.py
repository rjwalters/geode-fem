"""Memo-skill asset resolver for the per-company theme primitive (#322).

Phase A of #322 ships **memo only**: a small precedence walker that
hands the memo render pipeline the right ``template.html`` /
``styles.css`` / ``template.tex`` path given an optional theme name.
Phase B (deferred to per-skill follow-up issues) lifts the same pattern
to the other 7 artifact skills.

Precedence (per the #322 design)::

    <consumer>/.anvil/themes/<theme>/memo/<asset>
        >  <consumer>/.anvil/anvil/lib/memo/<asset>  [implicit — see note]
        >  framework default at anvil/lib/memo/<asset>

The middle "consumer single-tenant override" tier deserves a footnote:
post-#230, anvil installs at ``<consumer>/.anvil/anvil/lib/`` and the
``_render.__file__``-rooted framework-default lookup automatically
points there. So a consumer who edits ``<consumer>/.anvil/anvil/lib/
memo/styles.css`` in-place is editing the file the framework default
resolves to — there's no separate "single-tenant override" path the
resolver needs to walk. This module just adds the **theme tier** above
the framework default. The behavior is identical to pre-#322 when no
theme is declared or no per-skill override is found.

Public API
----------
- :func:`resolve_memo_asset` — return the path to one of ``template.html``,
  ``styles.css``, or ``template.tex`` honoring the precedence above.
- :data:`MEMO_ASSET_NAMES` — the three asset filenames the resolver knows.

The resolver does **not** read the file's contents; the caller (the
memo render path in :mod:`anvil.lib.render_gate`) passes the returned
path to pandoc's ``--template`` / ``--css`` flags directly.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional

from anvil.lib.theme import THEMES_DIRNAME, ANVIL_DIRNAME


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MEMO_ASSET_TEMPLATE_HTML = "template.html"
MEMO_ASSET_STYLES_CSS = "styles.css"
MEMO_ASSET_TEMPLATE_TEX = "template.tex"

MEMO_ASSET_NAMES = (
    MEMO_ASSET_TEMPLATE_HTML,
    MEMO_ASSET_STYLES_CSS,
    MEMO_ASSET_TEMPLATE_TEX,
)
"""The three memo asset filenames this resolver knows.

The set is closed for Phase A. Future memo-skill assets (e.g., a
pinned reference.docx for docx output) would be added here; until
then, callers should pass one of these constants.
"""

# Skill name used for the theme directory layout
# (``<consumer>/.anvil/themes/<theme>/memo/<asset>``).
MEMO_SKILL_DIRNAME = "memo"


# ---------------------------------------------------------------------------
# Framework-default location
# ---------------------------------------------------------------------------


def _framework_default_dir() -> Path:
    """Return the directory holding the shipped memo template assets.

    Resolved relative to ``anvil.lib.render`` so that whether anvil is
    running from a source checkout (``anvil/lib/memo/``) or from an
    installed consumer (``<consumer>/.anvil/anvil/lib/memo/``), the
    correct path comes back.

    Imported lazily to keep this module importable when render-time
    optional deps are missing (the render module itself doesn't import
    anything heavy at module load, but the lazy import mirrors the
    pattern in ``render_gate.py`` for symmetry).
    """
    from anvil.lib import render as _render

    return Path(_render.__file__).parent / "memo"


# ---------------------------------------------------------------------------
# Resolver
# ---------------------------------------------------------------------------


def resolve_memo_asset(
    asset_name: str,
    *,
    consumer_root: Optional[Path],
    theme_name: Optional[str],
) -> Path:
    """Return the path to a memo template asset, honoring theme precedence.

    Parameters
    ----------
    asset_name
        One of ``"template.html"``, ``"styles.css"``, ``"template.tex"``.
        See :data:`MEMO_ASSET_NAMES`.
    consumer_root
        The consumer repo root (the directory containing ``.anvil/``).
        ``None`` means "no consumer root located" — common in tests
        running from a temp dir without an installed anvil — in which
        case only the framework default is consulted.
    theme_name
        The theme name from the project BRIEF's ``theme:`` field.
        ``None`` means "no theme declared", which also short-circuits
        to the framework default. The resolver does NOT raise on an
        unknown theme name; it falls through silently.

    Returns
    -------
    Path
        Absolute path to the resolved asset. The framework default is
        guaranteed to exist (it ships with anvil), so the function
        always returns *some* valid path even when the theme tier
        doesn't apply.

    Raises
    ------
    ValueError
        If ``asset_name`` is not in :data:`MEMO_ASSET_NAMES`. The
        resolver is a closed-set API; misspellings should be loud.

    Examples
    --------
    Theme declared, theme dir present, asset present:

    >>> resolve_memo_asset(
    ...     "styles.css",
    ...     consumer_root=Path("/repo"),
    ...     theme_name="sphere-semi",
    ... )
    PosixPath('/repo/.anvil/themes/sphere-semi/memo/styles.css')

    Theme declared but no theme dir → framework default:

    >>> resolve_memo_asset(
    ...     "styles.css",
    ...     consumer_root=Path("/repo"),
    ...     theme_name="missing-theme",
    ... )
    PosixPath('.../anvil/lib/memo/styles.css')

    No theme declared (``theme_name=None``) → framework default:

    >>> resolve_memo_asset(
    ...     "styles.css", consumer_root=Path("/repo"), theme_name=None,
    ... )
    PosixPath('.../anvil/lib/memo/styles.css')
    """
    if asset_name not in MEMO_ASSET_NAMES:
        raise ValueError(
            f"Unknown memo asset {asset_name!r}; expected one of "
            f"{MEMO_ASSET_NAMES}. The resolver is closed-set; if you "
            f"need a new memo asset, add it to MEMO_ASSET_NAMES and "
            f"document it in anvil/lib/memo/README.md."
        )

    # Tier 1: per-theme override.
    if consumer_root is not None and theme_name is not None and str(theme_name).strip():
        theme_asset = (
            Path(consumer_root)
            / ANVIL_DIRNAME
            / THEMES_DIRNAME
            / theme_name
            / MEMO_SKILL_DIRNAME
            / asset_name
        )
        if theme_asset.is_file():
            return theme_asset

    # Tier 2: framework default (post-#230, this is also the
    # consumer-installed in-place override path — anvil is installed at
    # ``<consumer>/.anvil/anvil/lib/memo/`` and the framework-default
    # lookup automatically resolves there).
    return _framework_default_dir() / asset_name


__all__ = [
    "MEMO_ASSET_NAMES",
    "MEMO_ASSET_STYLES_CSS",
    "MEMO_ASSET_TEMPLATE_HTML",
    "MEMO_ASSET_TEMPLATE_TEX",
    "MEMO_SKILL_DIRNAME",
    "resolve_memo_asset",
]
