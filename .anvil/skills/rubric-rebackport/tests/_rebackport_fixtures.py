"""Programmatic fixture builders for `anvil:rubric-rebackport` tests (issue #358).

The skill's fixtures are project trees the tests construct in tmp
directories rather than baked-on-disk snapshots. This keeps the repo
small and the fixtures readable next to the tests that consume them.

Builders cover the four named fixtures from the curator notes:

- ``build_legacy_unstamped`` — single /40 memo thread with one
  reviewer sibling whose ``_meta.json`` lacks rubric stamping
  everywhere.
- ``build_partially_stamped`` — single /40 memo thread with a
  ``_meta.json`` already stamped but the ``_progress.json``
  ``score_history[]`` rows not.
- ``build_fully_stamped`` — single thread where every file is
  already stamped (no-op input).
- ``build_mixed_skill_portfolio`` — memo + proposal threads with
  mixed stamping. Exercises ``--skill=`` scoping.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Optional


def _write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _meta_legacy() -> dict:
    """Return a legacy reviewer ``_meta.json`` (no stamping fields)."""
    return {
        "critic": "review",
        "role": "memo-review.md",
        "started": "2026-05-01T12:00:00Z",
        "finished": "2026-05-01T12:05:00Z",
        "model": "claude-opus-4-1",
        "schema_version": 1,
        "scorecard_kind": "human-verdict",
        # Note: rubric_total IS present here so the heuristic can fire
        # without --legacy-rubric.
        "rubric_total": 40,
    }


def _meta_stamped_v1() -> dict:
    """Return a fully-stamped /40 ``_meta.json``."""
    m = _meta_legacy()
    m["rubric_id"] = "anvil-memo-v1-legacy-40"
    m["advance_threshold"] = 32
    return m


def _progress_legacy(thread: str) -> dict:
    """Return a legacy ``_progress.json`` whose score_history rows lack rubric_id."""
    return {
        "version": 1,
        "thread": thread,
        "phases": {
            "review": {"state": "done"},
        },
        "metadata": {
            "iteration": 1,
            "max_iterations": 4,
            "score_history": [
                {"iteration": 1, "total": 30, "threshold": 32},
            ],
        },
    }


def _progress_stamped(thread: str) -> dict:
    """Return a fully-stamped ``_progress.json``."""
    p = _progress_legacy(thread)
    for row in p["metadata"]["score_history"]:
        row["rubric_id"] = "anvil-memo-v1-legacy-40"
    return p


def _brief_for_skill(slug: str, artifact_type: str) -> str:
    """Return a minimal project BRIEF.md text with the given slug + type."""
    return (
        "---\n"
        f"project: {slug}-project\n"
        "audience: []\n"
        "hard_rules: []\n"
        "documents:\n"
        f"  - slug: {slug}\n"
        f"    artifact_type: {artifact_type}\n"
        "---\n"
        "\n"
        "# Project BRIEF\n"
    )


# ---------------------------------------------------------------------------
# Fixture: legacy_unstamped
# ---------------------------------------------------------------------------


def build_legacy_unstamped(
    root: Path,
    project_name: str = "legacy-memo",
    *,
    slug: str = "memo",
) -> Path:
    """Build a single /40 memo thread whose review is fully unstamped.

    Shape:
      <project>/
        BRIEF.md
        memo/
          memo.1/
            memo.md
            _progress.json
          memo.1.review/
            _meta.json
            _summary.md
            verdict.md
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)
    _write(project_dir / "BRIEF.md", _brief_for_skill(slug, "investment-memo"))

    thread_dir = project_dir / slug
    v1 = thread_dir / f"{slug}.1"
    _write(v1 / "memo.md", "# memo v1\n\nBody.\n")
    _write(
        v1 / "_progress.json",
        json.dumps(_progress_legacy(slug), indent=2) + "\n",
    )

    review_dir = thread_dir / f"{slug}.1.review"
    _write(
        review_dir / "_meta.json",
        json.dumps(_meta_legacy(), indent=2) + "\n",
    )
    _write(
        review_dir / "_summary.md",
        "---\n"
        "for_version: 1\n"
        "scorecard_kind: human-verdict\n"
        "critical_flag: false\n"
        "---\n"
        "\n"
        "# Review summary\n\nLegacy summary body.\n",
    )
    _write(review_dir / "verdict.md", "# Verdict\n\nLegacy verdict.\n")
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: partially_stamped
# ---------------------------------------------------------------------------


def build_partially_stamped(
    root: Path,
    project_name: str = "partial-memo",
    *,
    slug: str = "memo",
) -> Path:
    """Build a thread whose `_meta.json` is stamped but progress rows are not."""
    project_dir = build_legacy_unstamped(root, project_name, slug=slug)
    review_dir = project_dir / slug / f"{slug}.1.review"
    # Overwrite the _meta.json with a stamped variant.
    _write(
        review_dir / "_meta.json",
        json.dumps(_meta_stamped_v1(), indent=2) + "\n",
    )
    # The progress file is still legacy (rows lack rubric_id).
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: fully_stamped
# ---------------------------------------------------------------------------


def build_fully_stamped(
    root: Path,
    project_name: str = "stamped-memo",
    *,
    slug: str = "memo",
) -> Path:
    """Build a thread that's fully stamped — should be a no-op for the tool."""
    project_dir = build_legacy_unstamped(root, project_name, slug=slug)
    review_dir = project_dir / slug / f"{slug}.1.review"
    _write(
        review_dir / "_meta.json",
        json.dumps(_meta_stamped_v1(), indent=2) + "\n",
    )
    # Stamp the progress file's score_history rows.
    v1 = project_dir / slug / f"{slug}.1"
    _write(
        v1 / "_progress.json",
        json.dumps(_progress_stamped(slug), indent=2) + "\n",
    )
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: mixed_skill_portfolio
# ---------------------------------------------------------------------------


def build_mixed_skill_portfolio(
    root: Path,
    project_name: str = "portfolio",
) -> Path:
    """Build a portfolio with memo + proposal threads, both legacy unstamped.

    Shape:
      <project>/
        BRIEF.md          (documents: memo + proposal)
        memo/
          memo.1/memo.md + _progress.json
          memo.1.review/_meta.json + _summary.md + verdict.md
        proposal/
          proposal.1/proposal.md + _progress.json
          proposal.1.review/_meta.json + _summary.md + verdict.md
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)

    brief = (
        "---\n"
        f"project: {project_name}\n"
        "audience: []\n"
        "hard_rules: []\n"
        "documents:\n"
        "  - slug: memo\n"
        "    artifact_type: investment-memo\n"
        "  - slug: proposal\n"
        "    artifact_type: proposal\n"
        "---\n"
        "\n"
        "# Project BRIEF\n"
    )
    _write(project_dir / "BRIEF.md", brief)

    for slug, body_name in (("memo", "memo.md"), ("proposal", "proposal.md")):
        thread_dir = project_dir / slug
        v1 = thread_dir / f"{slug}.1"
        _write(v1 / body_name, f"# {slug} v1\n\nBody.\n")
        _write(
            v1 / "_progress.json",
            json.dumps(_progress_legacy(slug), indent=2) + "\n",
        )
        review_dir = thread_dir / f"{slug}.1.review"
        meta = _meta_legacy()
        meta["role"] = f"{slug}-review.md"
        _write(
            review_dir / "_meta.json",
            json.dumps(meta, indent=2) + "\n",
        )
        _write(
            review_dir / "_summary.md",
            "---\n"
            "for_version: 1\n"
            "scorecard_kind: human-verdict\n"
            "---\n"
            "\n"
            "# Review summary\n",
        )
        _write(review_dir / "verdict.md", "# Verdict\n")
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: pub_44_unstamped (post-#357 canary failure mode)
# ---------------------------------------------------------------------------


def _meta_legacy_pub_44() -> dict:
    """Return a /44-era pub reviewer ``_meta.json`` with rubric_total but no rubric_id."""
    return {
        "critic": "review",
        "role": "pub-review.md",
        "started": "2026-05-15T12:00:00Z",
        "finished": "2026-05-15T12:05:00Z",
        "model": "claude-opus-4-1",
        "schema_version": 1,
        "scorecard_kind": "human-verdict",
        # /44-era pub: rubric_total written, rubric_id absent. The
        # planner must heuristically pick `anvil-pub-v2` from the
        # (skill=pub, total=44) pair without --legacy-rubric.
        "rubric_total": 44,
    }


def build_pub_44_unstamped(
    root: Path,
    project_name: str = "legacy-pub-44",
    *,
    slug: str = "pub",
) -> Path:
    """Build a /44-era legacy-`pub` thread whose review is missing `rubric_id`.

    The thread carries a legacy `pub.md` body + `artifact_type: pub`
    BRIEF entry (the pre-#694 shape). Detection resolves both to the
    CURRENT skill name ``paper``, so this exercises the post-#357 catalog
    entry for ``("paper", 44)`` (the frozen ``anvil-pub-v2`` id) — the
    canary failure mode that motivated issue #366, plus the #694 legacy
    input-alias resolution.
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)
    _write(project_dir / "BRIEF.md", _brief_for_skill(slug, "pub"))

    thread_dir = project_dir / slug
    v1 = thread_dir / f"{slug}.1"
    _write(v1 / f"{slug}.md", f"# {slug} v1\n\nBody.\n")
    _write(
        v1 / "_progress.json",
        json.dumps(_progress_legacy(slug), indent=2) + "\n",
    )

    review_dir = thread_dir / f"{slug}.1.review"
    _write(
        review_dir / "_meta.json",
        json.dumps(_meta_legacy_pub_44(), indent=2) + "\n",
    )
    _write(
        review_dir / "_summary.md",
        "---\n"
        "for_version: 1\n"
        "scorecard_kind: human-verdict\n"
        "critical_flag: false\n"
        "---\n"
        "\n"
        "# Review summary\n\nLegacy /44-era pub summary body.\n",
    )
    _write(review_dir / "verdict.md", "# Verdict\n\nLegacy verdict.\n")
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: unconventional_body_filename_thread (issue #374 — canary repro)
# ---------------------------------------------------------------------------


def build_unconventional_body_filename_thread(
    root: Path,
    project_name: str = "canary-unconventional",
    *,
    thread_slug: str = "aldus",
    body_filename: str = "body.md",
    body_skill: str = "deck",
) -> Path:
    """Build a thread whose body filename is NOT in the inference table
    AND has no BRIEF — so the planner sees ``inferred_skill is None``.

    Reproduces the canary's #374 failure mode after the inference-table
    extension landed. The canary's actual repro was ``aldus/aldus.4/
    deck.md`` (no BRIEF), which the table-extension half of #374 fixes
    via rule 2. This fixture instead uses a body filename that is NOT
    in the (post-#374) extended table — ``body.md`` — so rule 2 still
    misses. Without the force-set-on-None semantics promoted by #374,
    this thread would be skipped with ``outside --skill=<deck> scope
    (inferred skill: None)`` even though the operator's assertion
    carries enough information to stamp.

    Shape (defaults):
      <project>/
        aldus/
          aldus.4/
            body.md             <- body filename NOT in inference table
            _progress.json
          aldus.4.review/
            _meta.json          <- /40-era pre-stamp shape
            _summary.md
            verdict.md

    Note: no BRIEF.md at the project root. This is intentional and
    matches the canary's actual on-disk state for portfolios that
    predate the post-#295/#296 BRIEF.md model.

    The ``body_filename`` and ``body_skill`` knobs let callers
    parameterize the fixture for other unconventional cases (e.g.,
    use ``body_filename="memo-body.md"`` to exercise the same code
    path under a memo-shaped operator assertion).
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)

    thread_dir = project_dir / thread_slug
    v = thread_dir / f"{thread_slug}.4"
    _write(v / body_filename, f"# {body_skill} v4\n\nBody.\n")
    _write(
        v / "_progress.json",
        json.dumps(_progress_legacy(thread_slug), indent=2) + "\n",
    )

    review_dir = thread_dir / f"{thread_slug}.4.review"
    meta = _meta_legacy()
    meta["role"] = f"{body_skill}-review.md"
    _write(
        review_dir / "_meta.json",
        json.dumps(meta, indent=2) + "\n",
    )
    _write(
        review_dir / "_summary.md",
        "---\n"
        "for_version: 4\n"
        "scorecard_kind: human-verdict\n"
        "critical_flag: false\n"
        "---\n"
        "\n"
        f"# Review summary\n\nLegacy {body_skill} summary body.\n",
    )
    _write(review_dir / "verdict.md", "# Verdict\n\nLegacy verdict.\n")
    return project_dir


def build_deck_thread_no_brief(
    root: Path,
    project_name: str = "canary-deck-no-brief",
    *,
    thread_slug: str = "aldus",
) -> Path:
    """Build the canary's exact #374 repro: deck thread, no BRIEF.

    Shape:
      <project>/
        aldus/
          aldus.4/
            deck.md             <- deck-fixed body filename
            _progress.json
          aldus.4.review/
            _meta.json          <- /40-era pre-stamp shape
            _summary.md
            verdict.md

    The #374 table extension (adding ``deck.md``,  ``slides.md``,
    ``ip-uspto.md`` to ``_BODY_FILENAME_TO_SKILL``) lets rule 2 of
    ``_infer_skill`` resolve this to ``"deck"`` even without a BRIEF.
    Used by ``test_deck_thread_no_brief_infers_via_body_filename``.
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)

    thread_dir = project_dir / thread_slug
    v = thread_dir / f"{thread_slug}.4"
    _write(v / "deck.md", "# deck v4\n\nBody.\n")
    _write(
        v / "_progress.json",
        json.dumps(_progress_legacy(thread_slug), indent=2) + "\n",
    )

    review_dir = thread_dir / f"{thread_slug}.4.review"
    meta = _meta_legacy()
    meta["role"] = "deck-review.md"
    _write(
        review_dir / "_meta.json",
        json.dumps(meta, indent=2) + "\n",
    )
    _write(
        review_dir / "_summary.md",
        "---\n"
        "for_version: 4\n"
        "scorecard_kind: human-verdict\n"
        "critical_flag: false\n"
        "---\n"
        "\n"
        "# Review summary\n\nLegacy deck summary body.\n",
    )
    _write(review_dir / "verdict.md", "# Verdict\n\nLegacy verdict.\n")
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: essay_44_unstamped (post-#366 catalog refresh — issue #482)
# ---------------------------------------------------------------------------


def _meta_essay_44_unstamped() -> dict:
    """Return an essay reviewer ``_meta.json`` with rubric_total but no rubric_id."""
    return {
        "critic": "review",
        "role": "essay-review.md",
        "started": "2026-06-01T12:00:00Z",
        "finished": "2026-06-01T12:05:00Z",
        "model": "claude-opus-4-1",
        "schema_version": 1,
        "scorecard_kind": "human-verdict",
        # Essay review missing its rubric_id stamp (e.g., a tool-edited
        # or hand-rolled review). The planner must heuristically pick
        # `anvil-essay-v1` from the (skill=essay, total=44) pair
        # without --legacy-rubric.
        "rubric_total": 44,
    }


def build_essay_44_unstamped(
    root: Path,
    project_name: str = "unstamped-essay-44",
    *,
    slug: str = "essay",
) -> Path:
    """Build an essay thread whose review is missing `rubric_id`.

    Exercises the post-#366 catalog entry for ``("essay", 44)`` —
    issue #482's second-occurrence catalog-drift repro. Skill
    inference goes through the BRIEF route (rule 1): the essay skill's
    body filename echoes the slug, so rule 2's fixed-filename table
    cannot fire.
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)
    _write(project_dir / "BRIEF.md", _brief_for_skill(slug, "essay"))

    thread_dir = project_dir / slug
    v1 = thread_dir / f"{slug}.1"
    _write(v1 / f"{slug}.md", f"# {slug} v1\n\nBody.\n")
    _write(
        v1 / "_progress.json",
        json.dumps(_progress_legacy(slug), indent=2) + "\n",
    )

    review_dir = thread_dir / f"{slug}.1.review"
    _write(
        review_dir / "_meta.json",
        json.dumps(_meta_essay_44_unstamped(), indent=2) + "\n",
    )
    _write(
        review_dir / "_summary.md",
        "---\n"
        "for_version: 1\n"
        "scorecard_kind: human-verdict\n"
        "critical_flag: false\n"
        "---\n"
        "\n"
        "# Review summary\n\nUnstamped essay summary body.\n",
    )
    _write(review_dir / "verdict.md", "# Verdict\n\nEssay verdict.\n")
    return project_dir


# ---------------------------------------------------------------------------
# Fixture: datasheet_44_unstamped (registry backfill — issue #486)
# ---------------------------------------------------------------------------


def _meta_datasheet_44_unstamped() -> dict:
    """Return a datasheet reviewer ``_meta.json`` with rubric_total but no rubric_id."""
    return {
        "critic": "review",
        "role": "datasheet-review.md",
        "started": "2026-06-01T12:00:00Z",
        "finished": "2026-06-01T12:05:00Z",
        "model": "claude-opus-4-1",
        "schema_version": 1,
        "scorecard_kind": "human-verdict",
        # Datasheet review missing its rubric_id stamp. The planner must
        # heuristically pick `anvil-datasheet-v1` from the
        # (skill=datasheet, total=44) pair without --legacy-rubric.
        "rubric_total": 44,
    }


def build_datasheet_44_unstamped(
    root: Path,
    project_name: str = "unstamped-datasheet-44",
    *,
    slug: str = "ax101-objdet",
) -> Path:
    """Build a datasheet thread whose review is missing `rubric_id`.

    Exercises the `("datasheet", 44)` catalog row (#484) through the
    BRIEF route (rule 1). The body is a fixed-name `datasheet.tex`,
    which is deliberately ABSENT from detect's
    `_BODY_FILENAME_TO_SKILL` table (BRIEF-route-only contract,
    matching `ip-uspto-provisional`'s `spec.tex`; issue #486), so
    rule-2 cannot fire — the only inference path is the registered
    `artifact_type: datasheet` BRIEF entry that #486 makes survive
    strict validation.
    """
    project_dir = root / project_name
    project_dir.mkdir(parents=True, exist_ok=True)
    _write(project_dir / "BRIEF.md", _brief_for_skill(slug, "datasheet"))

    thread_dir = project_dir / slug
    v1 = thread_dir / f"{slug}.1"
    # Fixed-name tex body — NOT slug-derived, NOT in rule-2 table.
    _write(v1 / "datasheet.tex", f"% {slug} v1\n\\documentclass{{anvil-datasheet}}\n")
    _write(
        v1 / "_progress.json",
        json.dumps(_progress_legacy(slug), indent=2) + "\n",
    )

    review_dir = thread_dir / f"{slug}.1.review"
    _write(
        review_dir / "_meta.json",
        json.dumps(_meta_datasheet_44_unstamped(), indent=2) + "\n",
    )
    _write(
        review_dir / "_summary.md",
        "---\n"
        "for_version: 1\n"
        "scorecard_kind: human-verdict\n"
        "critical_flag: false\n"
        "---\n"
        "\n"
        "# Review summary\n\nUnstamped datasheet summary body.\n",
    )
    _write(review_dir / "verdict.md", "# Verdict\n\nDatasheet verdict.\n")
    return project_dir


__all__ = [
    "build_datasheet_44_unstamped",
    "build_deck_thread_no_brief",
    "build_essay_44_unstamped",
    "build_fully_stamped",
    "build_legacy_unstamped",
    "build_mixed_skill_portfolio",
    "build_partially_stamped",
    "build_pub_44_unstamped",
    "build_unconventional_body_filename_thread",
]
