"""Pin test for the deck CSS ``section img`` max-height cap (issue #545).

The default ``anvil-deck`` theme caps ``section img { max-height: ... }``
to keep figures from overflowing the slide bounds. Issue #545 raised this
cap from ``60vh`` to ``75vh``: the prior 60vh value (~432px on a 1280x720
Marp slide) was tight enough that tall portrait PNGs (e.g., a 276x558 TB
flowchart) overflowed into adjacent bullets, forcing the reviser to
hand-patch ``h:`` Marp keywords into ``deck.md``.

This test pins the new cap so a future edit doesn't silently regress
back to 60vh.

Substring-presence only, no Marp render, no Pillow.
Deck-distinct filename per the #58 packaging convention.
"""

from __future__ import annotations

import re
from pathlib import Path

_HERE = Path(__file__).resolve().parent
_SKILL_ROOT = _HERE.parent  # anvil/skills/deck/

DECK_CSS = _SKILL_ROOT / "assets" / "anvil-deck.css"


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_deck_css_exists() -> None:
    assert DECK_CSS.exists(), f"anvil-deck.css missing at {DECK_CSS}"


def test_section_img_max_height_is_loosened() -> None:
    """The ``section img`` rule must declare ``max-height: 75vh`` (issue #545).

    The 60vh cap was the issue-#545 root cause for the second defect —
    tall portrait PNGs overflowed into adjacent bullets at that height.
    """
    body = _read(DECK_CSS)
    # Locate the section img rule block.
    match = re.search(
        r"section\s+img\s*\{[^}]*\}",
        body,
        flags=re.MULTILINE | re.DOTALL,
    )
    assert match is not None, (
        "Could not locate `section img { ... }` rule in anvil-deck.css. "
        "Issue #545 fix depends on this rule existing."
    )
    rule_body = match.group(0)
    assert "max-height: 75vh" in rule_body, (
        "anvil-deck.css `section img` rule must declare "
        "`max-height: 75vh` (issue #545 raised this from 60vh to give "
        "tall portrait PNGs room to render alongside bullets). "
        f"Rule body was:\n{rule_body}"
    )
    # And the prior 60vh must NOT come back as a regression.
    assert "max-height: 60vh" not in rule_body, (
        "anvil-deck.css `section img` rule still carries the old "
        "`max-height: 60vh` cap — issue #545 explicitly loosens this to "
        "75vh. A regression here re-introduces the bullet-overflow bug."
    )


def test_section_img_uses_object_fit_contain() -> None:
    """The rule should declare ``object-fit: contain`` to preserve aspect
    ratio when the image is constrained by the max-height cap."""
    body = _read(DECK_CSS)
    match = re.search(
        r"section\s+img\s*\{[^}]*\}",
        body,
        flags=re.MULTILINE | re.DOTALL,
    )
    assert match is not None
    rule_body = match.group(0)
    assert "object-fit: contain" in rule_body, (
        "anvil-deck.css `section img` rule should declare "
        "`object-fit: contain` so the image preserves aspect ratio when "
        "constrained by the max-height cap (issue #545)."
    )
