"""Doc-coverage smoke test for ``assets/imagery-style-presets.md``.

Issue #133 ships the backend-agnostic style preset library that
``deck-imagegen`` (Phase 2 of Epic #130) will consume. The library is
docs-only in v0, but the six preset keys are load-bearing: the BRIEF.md
``imagery_style:`` frontmatter field (Issue #132) parses against this
catalog, and the eventual composition logic dispatches on these keys.

This smoke test guards the doc-coverage contract from #133's acceptance
criteria:

- The file exists at the documented path.
- All six preset keys are documented as level-3 headings (``### `<key>` ``).
- Each preset documents the four required fields: key (the heading),
  intent (a ``**Intent**:`` line), prefix (a ``**Prefix**:`` line), and a
  worked example (a ``**Worked example**:`` line).
- The ``raw`` escape hatch is documented and its prefix is annotated as
  empty (we look for the literal "empty string" marker so a future edit
  cannot silently give ``raw`` a non-empty prefix without updating the
  marker).
- The file is backend-agnostic (no model-specific tokens like
  ``--ar``, ``midjourney``, ``stable diffusion``, ``flux``, ``dall-e``,
  ``dalle``, ``sdxl``, etc. appear in the catalog body — only as listed
  examples in the design-contract preamble).

Runs under ``pytest anvil/skills/deck/tests/test_imagery_style_presets_doc.py``.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

PRESETS_DOC = (
    Path(__file__).resolve().parents[1] / "assets" / "imagery-style-presets.md"
)

REQUIRED_KEYS = [
    "editorial-photography",
    "studio-product",
    "documentary",
    "diagram",
    "moodboard",
    "raw",
]


@pytest.fixture(scope="module")
def doc_text() -> str:
    """Read the preset library once per test module."""
    assert PRESETS_DOC.exists(), f"Preset library missing at {PRESETS_DOC}"
    return PRESETS_DOC.read_text(encoding="utf-8")


@pytest.fixture(scope="module")
def preset_sections(doc_text: str) -> dict[str, str]:
    """Split the doc into per-preset sections keyed by preset name.

    A section runs from its ``### `<key>``` heading to the next ``###`` or
    ``##`` heading (whichever comes first), exclusive. Returns a mapping
    of key -> section body (the heading is stripped).
    """
    # Match ``### `<key>``` headings; capture the key.
    heading_re = re.compile(r"^### `([a-z0-9_\-]+)`\s*$", re.MULTILINE)
    matches = list(heading_re.finditer(doc_text))
    sections: dict[str, str] = {}
    for i, m in enumerate(matches):
        key = m.group(1)
        body_start = m.end()
        # End at the next ``###`` or ``##`` heading.
        next_heading_re = re.compile(r"^#{2,3} ", re.MULTILINE)
        next_match = next_heading_re.search(doc_text, body_start)
        body_end = next_match.start() if next_match else len(doc_text)
        sections[key] = doc_text[body_start:body_end]
    return sections


def test_doc_exists(doc_text: str) -> None:
    """The preset library file is present and non-empty."""
    assert doc_text.strip(), "Preset library is empty"


def test_all_six_preset_keys_documented(preset_sections: dict[str, str]) -> None:
    """All six v0 preset keys appear as ``### `<key>``` headings."""
    missing = [k for k in REQUIRED_KEYS if k not in preset_sections]
    assert not missing, (
        f"Preset keys missing from {PRESETS_DOC.name}: {missing}. "
        f"Found keys: {sorted(preset_sections)}"
    )


@pytest.mark.parametrize("preset", REQUIRED_KEYS)
def test_preset_has_required_fields(
    preset: str, preset_sections: dict[str, str]
) -> None:
    """Each preset section documents intent, prefix, and worked example.

    The four required fields per the #133 acceptance criteria are: key
    (the heading itself; checked separately), intent, prefix, and worked
    example.
    """
    assert preset in preset_sections, f"Preset '{preset}' missing"
    body = preset_sections[preset]
    assert "**Intent**:" in body, (
        f"Preset '{preset}' missing **Intent**: field"
    )
    assert "**Prefix**:" in body, (
        f"Preset '{preset}' missing **Prefix**: field"
    )
    assert "**Worked example**:" in body, (
        f"Preset '{preset}' missing **Worked example**: field"
    )


def test_raw_preset_documents_empty_prefix(
    preset_sections: dict[str, str],
) -> None:
    """``raw`` is the escape hatch — its prefix MUST be documented as empty.

    We grep for the literal "empty string" marker so a future edit cannot
    silently give ``raw`` a non-empty prefix without also updating the
    marker (and thereby failing this test).
    """
    body = preset_sections["raw"]
    # Find the **Prefix**: line and confirm "empty string" appears in its
    # immediate vicinity.
    prefix_idx = body.find("**Prefix**:")
    assert prefix_idx >= 0
    # Look at the next ~200 chars for the empty-string marker.
    window = body[prefix_idx : prefix_idx + 200]
    assert "empty string" in window.lower(), (
        "raw preset prefix must be documented as an empty string (escape "
        "hatch); marker 'empty string' not found near the **Prefix**: "
        "line in the raw section"
    )


def test_no_backend_specific_tokens_in_preset_catalog(
    preset_sections: dict[str, str],
) -> None:
    """Presets must be backend-agnostic — no model-specific tokens.

    The design contract is that presets describe intent and adapters
    translate intent into backend-specific execution. A preset section
    that mentions ``--ar`` (Midjourney), ``sdxl`` (Stable Diffusion XL),
    a Flux-specific style token, etc. would leak backend semantics into
    a layer that is supposed to be portable.

    Note: model names ARE allowed in the design-contract preamble and
    references (where we say things like "regardless of which backend a
    consumer registers — Flux, DALL-E, ..."); we only inspect the
    per-preset sections returned by ``preset_sections``.
    """
    forbidden = [
        "midjourney",
        "--ar",
        "stable diffusion",
        "sdxl",
        "flux",
        "dall-e",
        "dalle",
    ]
    offenders: dict[str, list[str]] = {}
    for key, body in preset_sections.items():
        body_lower = body.lower()
        hits = [tok for tok in forbidden if tok in body_lower]
        if hits:
            offenders[key] = hits
    assert not offenders, (
        "Backend-specific tokens leaked into preset catalog sections "
        f"(presets must be backend-agnostic): {offenders}"
    )


def test_skill_md_references_preset_library() -> None:
    """SKILL.md mentions the preset library so consumers can find it."""
    skill_md = PRESETS_DOC.parents[1] / "SKILL.md"
    assert skill_md.exists(), f"SKILL.md missing at {skill_md}"
    text = skill_md.read_text(encoding="utf-8")
    assert "imagery-style-presets.md" in text, (
        "SKILL.md must reference assets/imagery-style-presets.md so the "
        "preset library is discoverable from the skill root"
    )
