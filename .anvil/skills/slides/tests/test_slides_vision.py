"""Tests for the slides-vision critic wiring.

This exercises the slides skill's vision integration. The slides skill
reuses the framework `VisionCritic` primitive (`anvil/lib/vision.py`) and
the shared Marp render path (`anvil/lib/render.py`); this issue (#45) adds
the per-skill `slides-vision.md` command, a smoke fixture, and this test.

The VLM call is stubbed with callbacks that simulate the expected
detection. Real Anthropic calls are out of scope for this test (see
``tests/lib/test_vision.py`` in the repo root for the opt-in smoke path).

The fixture reproduces the bug pattern at the markdown-source level even
though rendered defects cannot literally be observed without running
Marp; each stub callback encodes the expected vision detection for that
fixture's failure mode.

The test module is intentionally named ``test_slides_vision.py`` (not the
generic ``test_vision.py``) so it never collides with the deck skill's
``test_deck_vision.py`` when both ``deck/tests/`` and ``slides/tests/``
are collected under a single pytest rootdir.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Dict

import pytest


# Ensure repo root is importable. This file lives at
# anvil/skills/slides/tests/test_slides_vision.py — four levels deep from
# the repo root (slides/tests/<file> -> slides -> skills -> anvil -> root).
_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parents[3]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from anvil.lib.critics import (  # noqa: E402
    aggregate,
    discover_critics,
    load_review,
)
from anvil.lib.review_schema import Kind, Verdict  # noqa: E402
from anvil.lib.vision import (  # noqa: E402
    CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING,
    CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
    DEFAULT_VISION_DIMENSIONS,
    VisionCritic,
    default_vision_rubric,
)


FIXTURES = _HERE / "fixtures" / "vision"


def _clean_score_row(dim_name: str, max: int) -> Dict:
    return {
        "dimension": dim_name,
        "score": max - 1,
        "critical": False,
        "justification": "Default clean score for this fixture.",
        "fix": None,
    }


def _baseline_payload() -> Dict:
    """A "all clean" payload that individual fixtures perturb."""
    return {
        "scores": [
            _clean_score_row(d.name, d.max) for d in DEFAULT_VISION_DIMENSIONS
        ],
        "findings": [],
        "critical_flags": [],
    }


# ---------------------------------------------------------------------------
# Fixture presence
# ---------------------------------------------------------------------------


def test_fixture_deck_present():
    """The slides-vision smoke fixture exists under the fixtures dir."""
    assert (FIXTURES / "repro_mathtext_overflow.md").exists()


# ---------------------------------------------------------------------------
# MathJax `$`-as-math failure mode + rendered overflow on a dense slide
# ---------------------------------------------------------------------------


def _make_stub_for_mathtext_overflow(images, prompt):
    """Stub returning the expected detection for the slides repro.

    The fixture quotes a literal dollar amount (``$4M``) on a talk slide
    rendered with ``math: mathjax``; the bare ``$`` opens a math span and
    the amount renders as italic ``4M``. The slide is also dense (display
    equation + 3 bullets + source line), so vertical_overflow/slide_density
    score low.
    """
    payload = _baseline_payload()
    for s in payload["scores"]:
        if s["dimension"] == "mathtext_artifacts":
            s["score"] = 0
            s["critical"] = True
            s["justification"] = (
                "'$4M' on the compute-spend bullet renders as italic '4M' "
                "(the bare $ opens a MathJax math span). Load-bearing "
                "semantic loss for a cost slide in a technical talk."
            )
            s["fix"] = (
                "Escape the dollar sign as `\\$4M`, or use a non-dollar "
                "formatting (e.g. 'USD 4M')."
            )
        if s["dimension"] == "vertical_overflow":
            s["score"] = 2
            s["justification"] = (
                "Display equation + 3 bullets + source line on 16:9: the "
                "source line clips below the safe area at projection scale."
            )
            s["fix"] = "Move the source line into presenter notes."
        if s["dimension"] == "slide_density":
            s["score"] = 2
            s["justification"] = (
                "Display equation + 3 bullets + source line exceeds the "
                "~6-element working bar for a talk slide."
            )
    payload["critical_flags"].append(
        {
            "type": CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING,
            "justification": (
                "The compute-spend figure rendered without its dollar "
                "sign; the audience would not parse '4M' as a currency "
                "amount, and the slide's whole point is the cost."
            ),
            "evidence_span": "deck.pdf:slide=1",
        }
    )
    payload["findings"].append(
        {
            "severity": "major",
            "dimension": "mathtext_artifacts",
            "rationale": "Slide 1 'Cost of training': '$4M' renders as italic '4M'.",
            "suggested_fix": "Escape the dollar or use 'USD 4M'.",
            "evidence_span": "deck.pdf:slide=1",
        }
    )
    payload["findings"].append(
        {
            "severity": "minor",
            "dimension": "vertical_overflow",
            "rationale": "Slide 1 source line clips below the 16:9 safe area.",
            "suggested_fix": "Move the source line into presenter notes.",
            "evidence_span": "deck.pdf:slide=1",
        }
    )
    return payload


def test_vision_detects_mathtext_artifact(tmp_path):
    """slides-vision asserts the expected detection for the repro fixture."""
    fixture_image = tmp_path / "page-1.png"
    fixture_image.write_bytes(b"\x89PNG fake")

    critic = VisionCritic(
        critic_id="slides-vision",
        callback=_make_stub_for_mathtext_overflow,
    )
    review = critic.critique(
        images=[fixture_image],
        rubric=default_vision_rubric(),
        version_dir="kdd-2026.1",
        rendered_artifact="deck.pdf",
        context="This is a 1-slide conference talk deck.",
    )

    # The review is a proper vision review wired to the slides critic id.
    assert review.kind == Kind.VISION
    assert review.critic_id == "slides-vision"
    assert review.rendered_artifact == "deck.pdf"

    # mathtext_artifacts dim was scored 0 with critical=True.
    mathtext = next(
        s for s in review.scores if s.dimension == "mathtext_artifacts"
    )
    assert mathtext.score == 0
    assert mathtext.critical is True

    # The MathJax critical flag with the expected type is present.
    flags = [cf.type for cf in review.critical_flags]
    assert CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING in flags


def test_vision_detects_rendered_overflow_flag(tmp_path):
    """The overflow critical-flag type is raised when load-bearing info clips."""
    fixture_image = tmp_path / "page-1.png"
    fixture_image.write_bytes(b"\x89PNG fake")

    def _stub(images, prompt):
        payload = _baseline_payload()
        for s in payload["scores"]:
            if s["dimension"] == "vertical_overflow":
                s["score"] = 1
                s["critical"] = True
                s["justification"] = (
                    "The final result line ('effective cost ≈ $4.9M') is "
                    "clipped below the safe area; the headline number of "
                    "the slide is lost."
                )
        payload["critical_flags"].append(
            {
                "type": CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE,
                "justification": (
                    "The effective-cost result is the load-bearing number "
                    "of the slide and is clipped off-screen."
                ),
                "evidence_span": "deck.pdf:slide=1",
            }
        )
        return payload

    critic = VisionCritic(critic_id="slides-vision", callback=_stub)
    review = critic.critique(
        images=[fixture_image],
        rubric=default_vision_rubric(),
        version_dir="kdd-2026.1",
        rendered_artifact="deck.pdf",
    )

    overflow = next(
        s for s in review.scores if s.dimension == "vertical_overflow"
    )
    assert overflow.score == 1
    assert overflow.critical is True
    flags = [cf.type for cf in review.critical_flags]
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in flags


# ---------------------------------------------------------------------------
# Discovery + aggregation (acceptance criterion #3)
# ---------------------------------------------------------------------------


def _write_vision_sibling(tmp_path, callback) -> Path:
    """Write a `<thread>.1.vision/_review.json` to disk and return version dir."""
    portfolio = tmp_path
    version_dir = portfolio / "kdd-2026.1"
    version_dir.mkdir()
    vision_sibling = portfolio / "kdd-2026.1.vision"
    vision_sibling.mkdir()
    (vision_sibling / "slides").mkdir()

    fake_png = vision_sibling / "slides" / "page-1.png"
    fake_png.write_bytes(b"\x89PNG fake")

    critic = VisionCritic(critic_id="slides-vision", callback=callback)
    review = critic.critique(
        images=[fake_png],
        rubric=default_vision_rubric(),
        version_dir="kdd-2026.1",
        rendered_artifact="deck.pdf",
    )
    (vision_sibling / "_review.json").write_text(
        review.model_dump_json(indent=2)
    )
    return version_dir


def test_vision_sibling_is_discovered(tmp_path):
    """discover_critics finds the `.vision/` sibling by its canonical _review.json."""
    version_dir = _write_vision_sibling(
        tmp_path, _make_stub_for_mathtext_overflow
    )

    discovered = discover_critics(version_dir)
    names = [p.name for p in discovered]
    assert "kdd-2026.1.vision" in names


def test_vision_sibling_loads_and_aggregates(tmp_path):
    """load_review parses the vision sibling and aggregate consumes it cleanly."""
    version_dir = _write_vision_sibling(
        tmp_path, _make_stub_for_mathtext_overflow
    )

    discovered = discover_critics(version_dir)
    reviews = [load_review(d) for d in discovered]
    assert reviews
    assert any(r.kind == Kind.VISION for r in reviews)

    agg = aggregate(reviews)

    # The six vision dims survive into the aggregated scorecard.
    agg_dims = {s.dimension for s in agg.scores}
    for d in DEFAULT_VISION_DIMENSIONS:
        assert d.name in agg_dims

    # The mathtext critical flag forces a BLOCK verdict through aggregation.
    assert agg.verdict == Verdict.BLOCK
    flag_types = {cf.type for cf in agg.critical_flags}
    assert CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING in flag_types


# ---------------------------------------------------------------------------
# Command spec presence (acceptance criterion #1)
# ---------------------------------------------------------------------------


def test_slides_vision_command_spec_exists():
    """anvil/skills/slides/commands/slides-vision.md is present and complete."""
    cmd = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "slides"
        / "commands"
        / "slides-vision.md"
    )
    assert cmd.exists()
    text = cmd.read_text()
    # The six owned dims are documented.
    for d in DEFAULT_VISION_DIMENSIONS:
        assert d.name in text
    # The two shipped critical-flag types are documented.
    assert CRITICAL_FLAG_RENDERED_OVERFLOW_UNRECOVERABLE in text
    assert CRITICAL_FLAG_MATHTEXT_ARTIFACT_BREAKS_MEANING in text
    # The progress/meta/review shapes are referenced.
    assert "_progress.json" in text
    assert "_meta.json" in text
    assert "_review.json" in text
    # Marp config pin reference.
    assert "config.yml" in text
    # Reuses the shared render helpers.
    assert "render_marp_to_pdf" in text
    assert "render_pdf_to_pngs" in text


def test_slides_revise_documents_vision_guidance():
    """slides-revise.md surfaces the D6 vision reviser-guidance note."""
    revise = (
        _REPO_ROOT
        / "anvil"
        / "skills"
        / "slides"
        / "commands"
        / "slides-revise.md"
    )
    text = revise.read_text()
    assert "slides-vision" in text
    # The note names the figure-source / mermaid fix path.
    assert "mermaid" in text.lower()
    assert "figures/src" in text
