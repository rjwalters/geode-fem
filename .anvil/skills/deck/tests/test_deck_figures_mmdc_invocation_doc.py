"""Doc-pin tests for the canonical ``mmdc`` invocation in ``deck-figures``
and ``slides-figures`` (issue #545).

The two figurer commands document an identical ``mmdc`` shell-out for
rendering Mermaid diagrams to PNG. Issue #545 surfaced that the
documented flag set (``--width 1600 --height 900``) is **insufficient**
on its own — those are mmdc's *viewport* dimensions, not the output
canvas; mmdc crops the PNG to the diagram's intrinsic bbox, so a sparse
``flowchart LR`` produces a wide-thin strip regardless of viewport.

The load-bearing fix is ``--scale 2`` (which doubles SVG pixel density
before PNG conversion, taking a 784×102 thin strip to 1568×204 — legible
at the deck theme's ``max-height`` cap). This test pins:

1. Both ``deck-figures.md`` and ``slides-figures.md`` document the
   canonical mmdc invocation with ``--scale 2`` present.
2. The existing flag set (``--input`` / ``--output`` / ``--width 1600``
   / ``--height 900`` / ``--backgroundColor white`` /
   ``-c anvil/lib/figures/mermaid-theme.json``) is preserved — the fix
   adds flags, it does not remove them.
3. Both commands carry the issue-#545 orientation guidance: prefer
   ``flowchart TB`` over ``flowchart LR`` for cyclic / dense diagrams.
4. The shared Python wrapper ``render_mermaid_to_png`` is mentioned as
   an alternative call path.

Substring-presence only, no subprocess, no real ``mmdc``. Follows the
precedent of ``test_deck_outline_command_doc.py``.

Deck-distinct filename per the #58 packaging convention.
"""

from __future__ import annotations

from pathlib import Path

_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]

DECK_FIGURES = _REPO_ROOT / "anvil" / "skills" / "deck" / "commands" / "deck-figures.md"
SLIDES_FIGURES = (
    _REPO_ROOT / "anvil" / "skills" / "slides" / "commands" / "slides-figures.md"
)


# The canonical flag set the figurer agent reads out of the doc. Each
# substring must appear in the mmdc invocation block of BOTH commands so
# the deck and slides skills stay in lockstep.
CANONICAL_MMDC_FLAGS = (
    "--input figures/src/<name>.mmd",
    "--output figures/<name>.png",
    "--width 1600",
    "--height 900",
    "--scale 2",
    "--backgroundColor white",
    "-c anvil/lib/figures/mermaid-theme.json",
)


def _read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def test_deck_figures_file_exists() -> None:
    assert DECK_FIGURES.exists(), f"deck-figures.md missing at {DECK_FIGURES}"


def test_slides_figures_file_exists() -> None:
    assert SLIDES_FIGURES.exists(), f"slides-figures.md missing at {SLIDES_FIGURES}"


def test_deck_figures_documents_canonical_mmdc_flags() -> None:
    """``deck-figures.md`` must document every flag in the canonical set."""
    body = _read(DECK_FIGURES)
    for flag in CANONICAL_MMDC_FLAGS:
        assert flag in body, (
            f"deck-figures.md is missing canonical mmdc flag {flag!r}. "
            "The fix for issue #545 adds --scale 2 to the documented "
            "invocation; the rest of the flag set must be preserved "
            "(the fix adds flags, it does not remove them)."
        )


def test_slides_figures_documents_canonical_mmdc_flags() -> None:
    """``slides-figures.md`` must document the same canonical flag set
    (lockstep with deck-figures.md)."""
    body = _read(SLIDES_FIGURES)
    for flag in CANONICAL_MMDC_FLAGS:
        assert flag in body, (
            f"slides-figures.md is missing canonical mmdc flag {flag!r}. "
            "The two figurer commands document an identical mmdc shell-out; "
            "issue #545 fix updates them in lockstep."
        )


def test_deck_figures_documents_scale_rationale() -> None:
    """The doc must explain WHY ``--scale 2`` is required (it is not just
    a stylistic preference — it is the legibility fix for the viewport /
    canvas mismatch in mmdc)."""
    body = _read(DECK_FIGURES)
    # Anchor on issue number + the core diagnostic phrase.
    assert "#545" in body, "deck-figures.md should reference issue #545"
    assert "viewport" in body.lower(), (
        "deck-figures.md should explain --scale by noting that --width/--height "
        "set the *viewport*, not the output canvas (the issue-#545 root cause)."
    )


def test_slides_figures_documents_scale_rationale() -> None:
    body = _read(SLIDES_FIGURES)
    assert "#545" in body, "slides-figures.md should reference issue #545"
    assert "viewport" in body.lower(), (
        "slides-figures.md should explain --scale by noting that --width/--height "
        "set the *viewport*, not the output canvas (the issue-#545 root cause)."
    )


def test_deck_figures_documents_orientation_guidance() -> None:
    """The doc must carry the TB-over-LR orientation guidance for cyclic
    / dense flowcharts (the authoring fix that ``--scale 2`` alone does
    not provide)."""
    body = _read(DECK_FIGURES)
    assert "flowchart TB" in body and "flowchart LR" in body, (
        "deck-figures.md should document the TB-over-LR orientation "
        "guidance for cyclic / dense flowcharts (issue #545)."
    )


def test_slides_figures_documents_orientation_guidance() -> None:
    body = _read(SLIDES_FIGURES)
    assert "flowchart TB" in body and "flowchart LR" in body, (
        "slides-figures.md should document the TB-over-LR orientation "
        "guidance for cyclic / dense flowcharts (issue #545)."
    )


def test_deck_figures_mentions_python_wrapper() -> None:
    """The figurer doc should mention the shared ``render_mermaid_to_png``
    Python wrapper as an alternative call path."""
    body = _read(DECK_FIGURES)
    assert "render_mermaid_to_png" in body, (
        "deck-figures.md should reference the shared Python wrapper "
        "anvil.lib.render.render_mermaid_to_png (issue #545 lib promotion)."
    )


def test_slides_figures_mentions_python_wrapper() -> None:
    body = _read(SLIDES_FIGURES)
    assert "render_mermaid_to_png" in body, (
        "slides-figures.md should reference the shared Python wrapper "
        "anvil.lib.render.render_mermaid_to_png (issue #545 lib promotion)."
    )
