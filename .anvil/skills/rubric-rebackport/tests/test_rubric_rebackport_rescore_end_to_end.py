"""End-to-end stub test for `anvil:rubric-rebackport` rescore mode across
all eight per-skill reviewer hooks (issue #368).

This test exercises the full rescore-apply flow that the per-skill
reviewer hooks enable:

1. Build a fixture project containing one legacy `<thread>.{N}.review/`
   per skill (8 skills total — memo, proposal, paper, deck, slides, report,
   ip-uspto, installation).
2. Run `apply_plan(plan, allow_rescore_subprocess=True)` — the
   rebackport tool walks the inventory, plans one rescore per legacy
   review, and writes the placeholder `_meta.json` carrying
   `rescore_state: "scheduled"` at each planned rescore sidecar path.
3. For each rescore spec, simulate the per-skill reviewer invocation by
   calling a stub that writes the full required-files manifest under the
   rescore sidecar dir + overwrites `_meta.json` with `rescore_state:
   "completed"` (the real reviewer command does this; we stub it here
   because invoking the LLM-backed slash command in a unit test is
   out of scope).

Then verify:

- Every planned `.review.rescore-<id>/` exists.
- Every sidecar's `_meta.json.rescore_state == "completed"` after the
  stub runs.
- Every sidecar's `_meta.json.prior_rubric_id` matches the operator-
  asserted `--legacy-rubric` (the legacy reviews in this fixture are
  unstamped — the rebackport tool's placeholder is the only source of
  `prior_rubric_id` until the reviewer overwrites it).
- The legacy `<thread>.{N}.review/` dir is byte-identical (no mutation;
  the contract is "sidecar never overwrites").
- `apply_plan` reports each rescore as written (not deferred) because
  all eight skill reviewer hooks ship the `--rescore-mode` token now.

Per `tests/_skill_lib.py`, this file loads the rubric-rebackport lib
modules under a unique package name to dodge the cross-skill ``lib``
collision.
"""

from __future__ import annotations

import hashlib
import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import apply_mod, detect, plan  # noqa: E402

apply_plan = apply_mod.apply_plan
inventory_tree = detect.inventory_tree
Mode = plan.Mode
build_plan = plan.build_plan


# Skill → (slug, body_filename, target_rubric_id, legacy_rubric_id, advance).
# The legacy_rubric_id values are the /40 (or /44 for ip-uspto) shapes
# from `KNOWN_RUBRICS` in `lib/plan.py`; the target shapes are the
# current rubric ids the rescore lands on.
SKILL_FIXTURE_SPECS = {
    "memo": (
        "memo",
        "memo.md",
        "anvil-memo-v2",
        "anvil-memo-v1-legacy-40",
    ),
    "proposal": (
        "proposal",
        "proposal.md",
        "anvil-proposal-v2",
        "anvil-proposal-v1-legacy-40",
    ),
    # The `pub` skill was renamed to `paper` under #694; the dict key,
    # thread slug, and directory name are the CURRENT skill name, while
    # the rubric_id literals stay the frozen `anvil-pub-v*` identities.
    "paper": (
        "paper",
        "main.tex",
        "anvil-pub-v2",
        "anvil-pub-v1",
    ),
    "deck": (
        "deck",
        "deck.md",
        "anvil-deck-v2",
        "anvil-deck-v1",
    ),
    "slides": (
        "slides",
        "deck.md",
        "anvil-slides-v2",
        "anvil-slides-v1",
    ),
    "report": (
        "report",
        "report.md",
        "anvil-report-v2",
        "anvil-report-v1",
    ),
    "ip-uspto": (
        "ip-uspto",
        "spec.tex",
        "anvil-ip-uspto-v2",
        "anvil-ip-uspto-v1",
    ),
    "installation": (
        "installation",
        "installation.tex",
        "anvil-installation-v2",
        "anvil-installation-v1-legacy-40",
    ),
}


def _write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _build_per_skill_portfolio(root: Path) -> Path:
    """Build a portfolio with one legacy review per skill.

    Each skill gets its own ``<skill>/`` subdirectory containing a
    ``<slug>.1/`` version dir + ``<slug>.1.review/`` review sibling.
    Each review's ``_meta.json`` is the legacy unstamped /40 (or /44 for
    ip-uspto) shape — no ``rubric_id`` field, so the rebackport
    planner needs ``--legacy-rubric`` to act.

    Returns the project dir.
    """
    project_dir = root / "multi-skill-portfolio"
    project_dir.mkdir(parents=True, exist_ok=True)

    # Compose a BRIEF.md that names every skill's slug. The detector
    # uses the BRIEF as the highest-confidence skill-inference path.
    brief_lines = [
        "---",
        "project: multi-skill-portfolio",
        "audience: []",
        "hard_rules: []",
        "documents:",
    ]
    for skill, (slug, _, _, _) in SKILL_FIXTURE_SPECS.items():
        artifact_type_map = {
            "memo": "investment-memo",
            "proposal": "proposal",
            "paper": "paper",
            "deck": "deck",
            "slides": "slides",
            "report": "report",
            "ip-uspto": "ip-uspto",
            "installation": "installation",
        }
        brief_lines.append(f"  - slug: {slug}")
        brief_lines.append(f"    artifact_type: {artifact_type_map[skill]}")
    brief_lines.append("---")
    brief_lines.append("")
    brief_lines.append("# Project BRIEF")
    brief_lines.append("")
    _write(project_dir / "BRIEF.md", "\n".join(brief_lines) + "\n")

    for skill, (slug, body_name, _target, _legacy) in SKILL_FIXTURE_SPECS.items():
        skill_dir = project_dir / skill
        v1 = skill_dir / f"{slug}.1"
        _write(v1 / body_name, f"# {slug} v1\n\nBody.\n")
        _write(
            v1 / "_progress.json",
            json.dumps(
                {
                    "version": 1,
                    "thread": slug,
                    "phases": {"review": {"state": "done"}},
                    "metadata": {
                        "iteration": 1,
                        "max_iterations": 4,
                        "score_history": [
                            {"iteration": 1, "total": 30, "threshold": 32}
                        ],
                    },
                },
                indent=2,
            )
            + "\n",
        )
        review_dir = skill_dir / f"{slug}.1.review"
        # Legacy unstamped meta — no rubric_id field; the rebackport
        # tool fills it via --legacy-rubric.
        meta = {
            "critic": "review",
            "role": f"{skill}-review.md",
            "started": "2026-05-01T12:00:00Z",
            "finished": "2026-05-01T12:05:00Z",
            "model": "claude-opus-4-1",
            "schema_version": 1,
            "scorecard_kind": (
                "machine-summary" if skill == "ip-uspto" else "human-verdict"
            ),
            "rubric_total": 45 if skill == "ip-uspto" else 40,
        }
        _write(review_dir / "_meta.json", json.dumps(meta, indent=2) + "\n")
        _write(
            review_dir / "_summary.md",
            "---\nfor_version: 1\nscorecard_kind: "
            f"{meta['scorecard_kind']}\ncritical_flag: false\n---\n\n"
            "# Review summary\n\nLegacy summary.\n",
        )
        _write(review_dir / "verdict.md", "# Verdict\n\nLegacy verdict.\n")

    return project_dir


def _simulate_reviewer_invocation(
    sidecar_path: Path, *, target_rubric_id: str, rescore_id: str
) -> None:
    """Stub the per-skill reviewer's rescore-mode invocation.

    The real reviewer command (`/anvil:<skill>-review --rescore-mode <id>`)
    would write the full required-files manifest. We stub this here by:

    1. Writing the canonical review files inside ``sidecar_path``.
    2. Overwriting ``_meta.json`` to flip ``rescore_state`` from
       ``"scheduled"`` (the placeholder the rebackport tool wrote) to
       ``"completed"`` and adding ``rescore_id``.

    The placeholder's other fields (``rubric_id``, ``rubric_total``,
    ``advance_threshold``, ``prior_rubric_id``, ``rescore_source``) are
    preserved verbatim.
    """
    assert sidecar_path.is_dir(), f"sidecar path missing: {sidecar_path}"
    meta_path = sidecar_path / "_meta.json"
    assert meta_path.is_file(), (
        f"placeholder _meta.json missing at {meta_path}"
    )
    placeholder = json.loads(meta_path.read_text())
    assert placeholder["rescore_state"] == "scheduled", (
        "placeholder must carry rescore_state=scheduled before reviewer "
        f"overwrites it; got {placeholder.get('rescore_state')!r}"
    )

    # Write the canonical review files. The exact contents don't matter
    # for the e2e stub — only their presence does.
    _write(sidecar_path / "verdict.md", "# Verdict\n\nRescored.\n")
    _write(sidecar_path / "scoring.md", "# Scoring\n\nDim scores.\n")
    _write(sidecar_path / "comments.md", "# Comments\n\nLine comments.\n")
    _write(
        sidecar_path / "_summary.md",
        "---\nrescored: true\n---\n\n# Review summary\n\nRescored.\n",
    )
    _write(
        sidecar_path / "_progress.json",
        json.dumps(
            {
                "version": 1,
                "phases": {"review": {"state": "done"}},
            },
            indent=2,
        )
        + "\n",
    )

    # Overwrite _meta.json: flip rescore_state, add rescore_id, preserve
    # the other placeholder fields verbatim.
    completed = dict(placeholder)
    completed["rescore_state"] = "completed"
    completed["rescore_id"] = rescore_id
    meta_path.write_text(json.dumps(completed, indent=2) + "\n", encoding="utf-8")


def _hash_tree(root: Path) -> dict:
    """Return a {relative-path: sha256} map for every file under root."""
    out: dict = {}
    for f in sorted(root.rglob("*")):
        if f.is_file():
            rel = str(f.relative_to(root))
            out[rel] = hashlib.sha256(f.read_bytes()).hexdigest()
    return out


class TestRescoreApplyEndToEndAcrossSkills(unittest.TestCase):
    def test_rescore_apply_writes_completed_sidecar_for_each_skill(
        self,
    ) -> None:
        with TemporaryDirectory() as td:
            project = _build_per_skill_portfolio(Path(td))

            # Pre-record legacy-review-dir hashes so we can assert
            # byte-identity at the end (the legacy dir must never be
            # mutated by a rescore pass).
            legacy_hashes_by_skill = {}
            for skill, (slug, _, _, _) in SKILL_FIXTURE_SPECS.items():
                legacy_dir = project / skill / f"{slug}.1.review"
                legacy_hashes_by_skill[skill] = _hash_tree(legacy_dir)

            # The rebackport tool requires `--legacy-rubric` for rescore
            # mode. The fixture has one review per skill with mixed
            # legacy ids — but `build_plan` accepts ONE legacy_rubric
            # for the whole run. We exercise the per-skill flow once per
            # skill with the matching legacy id, so each skill gets its
            # own apply pass.
            for skill, (slug, _, target_rubric_id, legacy_rubric_id) in (
                SKILL_FIXTURE_SPECS.items()
            ):
                with self.subTest(skill=skill):
                    inv = inventory_tree(project)
                    p = build_plan(
                        inv,
                        mode=Mode.RESCORE,
                        legacy_rubric=legacy_rubric_id,
                        skill_filter=skill,
                    )
                    # The skill_filter scopes to one review per pass.
                    skill_reviews = [
                        r for r in p.reviews if not r.skipped
                    ]
                    self.assertGreaterEqual(
                        len(skill_reviews),
                        1,
                        f"expected at least one non-skipped rescore plan "
                        f"for skill={skill}",
                    )
                    result = apply_plan(p, allow_rescore_subprocess=True)

                    # Verify the apply step recorded the rescore as
                    # WRITTEN (not deferred). This is the test that the
                    # `--rescore-mode` hook scan succeeded for this
                    # skill — `check_rescore_hook(skill)` returned True.
                    written_count = sum(
                        1
                        for o in result.outcomes
                        if o.rescore_outcome is not None
                        and o.rescore_outcome.written
                    )
                    deferred_count = sum(
                        1
                        for o in result.outcomes
                        if o.rescore_outcome is not None
                        and o.rescore_outcome.deferred
                    )
                    self.assertGreaterEqual(
                        written_count,
                        1,
                        f"skill={skill}: rescore must be recorded as "
                        f"written (the `--rescore-mode` hook is present "
                        f"per issue #368); got written={written_count}, "
                        f"deferred={deferred_count}",
                    )
                    self.assertEqual(
                        deferred_count,
                        0,
                        f"skill={skill}: no rescore should be deferred "
                        f"(post-#368, all 8 skill review commands "
                        f"carry the `--rescore-mode` token); got "
                        f"deferred={deferred_count}",
                    )

                    # For each rescore spec, simulate the per-skill
                    # reviewer invocation and verify the resulting
                    # sidecar shape.
                    for rp in skill_reviews:
                        if rp.rescore_spec is None:
                            continue
                        sidecar = rp.rescore_spec.sidecar_path
                        self.assertTrue(
                            sidecar.is_dir(),
                            f"skill={skill}: rescore sidecar missing at "
                            f"{sidecar}",
                        )
                        # Placeholder shape (before reviewer stub).
                        placeholder = json.loads(
                            (sidecar / "_meta.json").read_text()
                        )
                        self.assertEqual(
                            placeholder["rescore_state"],
                            "scheduled",
                            f"skill={skill}: placeholder must carry "
                            f"scheduled state; got "
                            f"{placeholder.get('rescore_state')!r}",
                        )
                        self.assertEqual(
                            placeholder["rubric_id"],
                            target_rubric_id,
                            f"skill={skill}: placeholder rubric_id must "
                            f"match the target rubric ({target_rubric_id})",
                        )
                        self.assertEqual(
                            placeholder["prior_rubric_id"],
                            legacy_rubric_id,
                            f"skill={skill}: placeholder prior_rubric_id "
                            f"must match the operator-asserted "
                            f"--legacy-rubric ({legacy_rubric_id})",
                        )
                        self.assertEqual(
                            placeholder["rescore_source"],
                            "anvil:rubric-rebackport",
                        )

                        # Simulate the per-skill reviewer's rescore-mode
                        # invocation.
                        _simulate_reviewer_invocation(
                            sidecar,
                            target_rubric_id=target_rubric_id,
                            rescore_id=target_rubric_id,
                        )

                        # Post-reviewer shape: rescore_state flipped to
                        # "completed", rescore_id added.
                        completed = json.loads(
                            (sidecar / "_meta.json").read_text()
                        )
                        self.assertEqual(
                            completed["rescore_state"],
                            "completed",
                            f"skill={skill}: reviewer stub failed to flip "
                            f"rescore_state to completed",
                        )
                        self.assertEqual(
                            completed["rescore_id"],
                            target_rubric_id,
                        )
                        # The placeholder's other fields are preserved.
                        self.assertEqual(
                            completed["prior_rubric_id"], legacy_rubric_id
                        )
                        self.assertEqual(
                            completed["rubric_id"], target_rubric_id
                        )
                        self.assertEqual(
                            completed["rescore_source"],
                            "anvil:rubric-rebackport",
                        )

            # Byte-identity check on every legacy review dir. The
            # rescore-apply pass must never mutate the legacy review
            # dir — the rescore is a sidecar write only.
            for skill, (slug, _, _, _) in SKILL_FIXTURE_SPECS.items():
                with self.subTest(skill=skill, check="legacy-byte-identity"):
                    legacy_dir = project / skill / f"{slug}.1.review"
                    after_hashes = _hash_tree(legacy_dir)
                    self.assertEqual(
                        legacy_hashes_by_skill[skill],
                        after_hashes,
                        f"skill={skill}: rescore-apply mutated the legacy "
                        f"review dir at {legacy_dir} — must be byte-identical",
                    )


if __name__ == "__main__":
    unittest.main()
