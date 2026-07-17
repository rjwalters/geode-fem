"""Tests for ``anvil.skills.rubric-rebackport.lib.plan`` (issue #358)."""

from __future__ import annotations

import json
import re
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))

from _skill_lib import detect, plan  # noqa: E402
from _rebackport_fixtures import (  # noqa: E402
    build_datasheet_44_unstamped,
    build_deck_thread_no_brief,
    build_essay_44_unstamped,
    build_fully_stamped,
    build_legacy_unstamped,
    build_mixed_skill_portfolio,
    build_partially_stamped,
    build_pub_44_unstamped,
    build_unconventional_body_filename_thread,
)

inventory_tree = detect.inventory_tree
CURRENT_RUBRIC_BY_SKILL = plan.CURRENT_RUBRIC_BY_SKILL
KNOWN_RUBRICS = plan.KNOWN_RUBRICS
Mode = plan.Mode
build_plan = plan.build_plan
infer_target_rubric_id = plan.infer_target_rubric_id
lookup_rubric_by_id = plan.lookup_rubric_by_id


class TestStampOnlyPlan(unittest.TestCase):
    def test_legacy_unstamped_plan_emits_all_three_edits(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertFalse(rp.skipped)
            self.assertIsNotNone(rp.rubric)
            self.assertIsNotNone(rp.stamp_meta)
            self.assertIsNotNone(rp.stamp_progress_rows)
            self.assertIsNotNone(rp.summary_block)

    def test_heuristic_inference_memo_40(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            rp = p.reviews[0]
            self.assertEqual(rp.rubric.id, "anvil-memo-v1-legacy-40")
            self.assertEqual(rp.rubric.total, 40)
            self.assertEqual(rp.rubric.advance_threshold, 32)

    def test_operator_assertion_overrides_heuristic(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.STAMP_ONLY,
                legacy_rubric="anvil-memo-v2",
            )
            rp = p.reviews[0]
            self.assertEqual(rp.rubric.id, "anvil-memo-v2")
            self.assertEqual(rp.rubric.total, 44)
            self.assertEqual(rp.rubric.advance_threshold, 35)

    def test_partially_stamped_plan_only_emits_progress_op(self) -> None:
        with TemporaryDirectory() as td:
            project = build_partially_stamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertIsNone(rp.stamp_meta)
            self.assertIsNotNone(rp.stamp_progress_rows)

    def test_fully_stamped_plan_is_noop(self) -> None:
        with TemporaryDirectory() as td:
            project = build_fully_stamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertTrue(rp.is_noop)

    def test_skill_filter_skips_offtarget(self) -> None:
        with TemporaryDirectory() as td:
            project = build_mixed_skill_portfolio(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv, mode=Mode.STAMP_ONLY, skill_filter="memo"
            )
            self.assertEqual(len(p.reviews), 2)
            memo_plan = next(
                r for r in p.reviews if r.skill == "memo"
            )
            proposal_plan = next(
                r for r in p.reviews if r.skill == "proposal"
            )
            self.assertFalse(memo_plan.skipped)
            self.assertTrue(proposal_plan.skipped)
            self.assertIn("outside", proposal_plan.skip_reason)


class TestRescorePlan(unittest.TestCase):
    def test_rescore_requires_legacy_rubric(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.RESCORE)
            rp = p.reviews[0]
            self.assertTrue(rp.skipped)
            self.assertIn("--legacy-rubric", rp.skip_reason)

    def test_rescore_emits_sidecar_spec(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
            )
            rp = p.reviews[0]
            self.assertFalse(rp.skipped)
            self.assertIsNotNone(rp.rescore_spec)
            self.assertEqual(
                rp.rescore_spec.target_rubric.id, "anvil-memo-v2"
            )
            expected_name = (
                rp.review_dir.name + ".rescore-anvil-memo-v2"
            )
            self.assertEqual(
                rp.rescore_spec.sidecar_path.name, expected_name
            )

    def test_rescore_noop_when_sidecar_exists(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            review_dir = inv.reviews[0].review_dir
            sidecar = (
                review_dir.parent
                / f"{review_dir.name}.rescore-anvil-memo-v2"
            )
            sidecar.mkdir()
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
            )
            legacy_review_id = inv.reviews[0].review_id
            rp_legacy = next(
                r for r in p.reviews if r.review_id == legacy_review_id
            )
            self.assertIsNone(
                rp_legacy.rescore_spec,
                "rescore should be no-op when sidecar already exists",
            )


class TestRubricCatalog(unittest.TestCase):
    def test_known_rubrics_cover_memo_and_proposal(self) -> None:
        self.assertIn(("memo", 40), KNOWN_RUBRICS)
        self.assertIn(("memo", 44), KNOWN_RUBRICS)
        self.assertIn(("proposal", 40), KNOWN_RUBRICS)
        self.assertIn(("proposal", 44), KNOWN_RUBRICS)

    def test_memo_v2_threshold_is_35(self) -> None:
        ri = KNOWN_RUBRICS[("memo", 44)]
        self.assertEqual(ri.id, "anvil-memo-v2")
        self.assertEqual(ri.advance_threshold, 35)

    def test_memo_v1_legacy_threshold_is_32(self) -> None:
        ri = KNOWN_RUBRICS[("memo", 40)]
        self.assertEqual(ri.id, "anvil-memo-v1-legacy-40")
        self.assertEqual(ri.advance_threshold, 32)

    def test_infer_target_rubric_id_handles_unknown_pair(self) -> None:
        self.assertIsNone(
            infer_target_rubric_id("unknown-skill", 40)
        )
        self.assertIsNone(
            infer_target_rubric_id("memo", 99)
        )
        self.assertIsNone(infer_target_rubric_id("memo", None))

    def test_lookup_rubric_by_id_round_trip(self) -> None:
        ri = lookup_rubric_by_id("anvil-memo-v2")
        self.assertIsNotNone(ri)
        self.assertEqual(ri.total, 44)
        self.assertIsNone(lookup_rubric_by_id("anvil-fake-v99"))

    # ---- Post-#357 /44 (and /45 for ip-uspto) catalog coverage (issue #366) ----

    def test_known_rubrics_cover_44_skills(self) -> None:
        """All 6 post-#357 (skill, total) pairs are in the catalog."""
        for skill, total in [
            ("paper", 44),
            ("report", 44),
            ("deck", 44),
            ("slides", 44),
            ("installation", 44),
            ("ip-uspto", 45),
        ]:
            self.assertIn((skill, total), KNOWN_RUBRICS)

    def test_paper_v2_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("paper", 44)]
        self.assertEqual(ri.id, "anvil-pub-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 35)

    def test_report_v2_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("report", 44)]
        self.assertEqual(ri.id, "anvil-report-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 39)

    def test_deck_v2_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("deck", 44)]
        self.assertEqual(ri.id, "anvil-deck-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 39)

    def test_slides_v2_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("slides", 44)]
        self.assertEqual(ri.id, "anvil-slides-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 35)

    def test_installation_v2_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("installation", 44)]
        self.assertEqual(ri.id, "anvil-installation-v2")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 35)

    def test_ip_uspto_v2_45_id_and_threshold(self) -> None:
        # ip-uspto is /45, not /44 — its rubric has an extra dimension.
        ri = KNOWN_RUBRICS[("ip-uspto", 45)]
        self.assertEqual(ri.id, "anvil-ip-uspto-v2")
        self.assertEqual(ri.total, 45)
        self.assertEqual(ri.advance_threshold, 39)

    def test_current_rubric_by_skill_points_at_44_for_migrated_skills(
        self,
    ) -> None:
        """CURRENT_RUBRIC_BY_SKILL must repoint at the post-#357 rubrics."""
        for skill in ("paper", "report", "deck", "slides", "installation"):
            self.assertEqual(
                CURRENT_RUBRIC_BY_SKILL[skill].total,
                44,
                f"`{skill}` current rubric must be /44 post-#357",
            )

    def test_current_rubric_by_skill_points_at_45_for_ip_uspto(self) -> None:
        self.assertEqual(CURRENT_RUBRIC_BY_SKILL["ip-uspto"].total, 45)
        self.assertEqual(
            CURRENT_RUBRIC_BY_SKILL["ip-uspto"].id, "anvil-ip-uspto-v2"
        )

    def test_current_rubric_by_skill_memo_and_proposal_unchanged(self) -> None:
        """Memo + proposal already targeted /44 pre-#366; no regression."""
        self.assertEqual(CURRENT_RUBRIC_BY_SKILL["memo"].total, 44)
        self.assertEqual(CURRENT_RUBRIC_BY_SKILL["proposal"].total, 44)

    def test_legacy_40_rows_retained_for_stamp_only_inference(self) -> None:
        """Adding /44 rows must NOT remove the /40 rows (legacy reviews still need them)."""
        for skill in (
            "paper",
            "report",
            "deck",
            "slides",
            "installation",
            "ip-uspto",
        ):
            self.assertIn(
                (skill, 40),
                KNOWN_RUBRICS,
                f"/40 row for `{skill}` removed — legacy reviews can no "
                "longer auto-infer.",
            )

    # ---- Post-#366 catalog refresh (issue #482) ----

    def test_known_rubrics_cover_post_366_skills(self) -> None:
        """All 3 post-#366 (skill, total) pairs are in the catalog."""
        for skill, total in [
            ("datasheet", 44),
            ("ip-uspto-provisional", 45),
            ("essay", 44),
        ]:
            self.assertIn((skill, total), KNOWN_RUBRICS)

    def test_datasheet_v1_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("datasheet", 44)]
        self.assertEqual(ri.id, "anvil-datasheet-v1")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 39)

    def test_ip_uspto_provisional_v1_45_id_and_threshold(self) -> None:
        # CAREFUL: the skill key is `ip-uspto-provisional` but the
        # rubric_id is `anvil-ip-provisional-v1` — NO "uspto" in the
        # id, per `anvil/skills/ip-uspto-provisional/rubric.md` line 3.
        # The asymmetry is the most likely transcription error, so the
        # literal is pinned here exactly (issue #482 curation note).
        ri = KNOWN_RUBRICS[("ip-uspto-provisional", 45)]
        self.assertEqual(ri.id, "anvil-ip-provisional-v1")
        self.assertEqual(ri.total, 45)
        self.assertEqual(ri.advance_threshold, 39)

    def test_essay_v1_44_id_and_threshold(self) -> None:
        ri = KNOWN_RUBRICS[("essay", 44)]
        self.assertEqual(ri.id, "anvil-essay-v1")
        self.assertEqual(ri.total, 44)
        self.assertEqual(ri.advance_threshold, 35)

    def test_current_rubric_by_skill_covers_post_366_skills(self) -> None:
        self.assertEqual(
            CURRENT_RUBRIC_BY_SKILL["datasheet"].id, "anvil-datasheet-v1"
        )
        self.assertEqual(
            CURRENT_RUBRIC_BY_SKILL["ip-uspto-provisional"].id,
            "anvil-ip-provisional-v1",
        )
        self.assertEqual(CURRENT_RUBRIC_BY_SKILL["essay"].id, "anvil-essay-v1")

    def test_no_essay_30_legacy_row(self) -> None:
        """The rjwalters.info blog /30 legacy row is deferred to #454.

        The legacy rubric_id literal for the consumer's 6-dim /30 blog
        rubric is not yet minted anywhere in this repo; adding a row
        here would risk diverging from the #454 review.md-conversion
        naming decision (issue #482 curation note).
        """
        self.assertNotIn(("essay", 30), KNOWN_RUBRICS)


class TestPub44AutoInference(unittest.TestCase):
    """End-to-end: /44-era pub review with `rubric_total: 44` but no
    `rubric_id` should resolve to `anvil-pub-v2` without `--legacy-rubric`.

    This is the canary failure mode that motivated issue #366: before
    the catalog gained the `("paper", 44)` entry, the planner would skip
    such reviews with a "heuristic miss" note instead of stamping them.
    """

    def test_pub_44_heuristic_inference_resolves_to_v2(self) -> None:
        with TemporaryDirectory() as td:
            project = build_pub_44_unstamped(Path(td))
            inv = inventory_tree(project)
            self.assertEqual(len(inv.reviews), 1)
            self.assertEqual(inv.reviews[0].inferred_skill, "paper")
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            rp = p.reviews[0]
            self.assertFalse(
                rp.skipped,
                f"Expected stamping plan; got skip with reason: "
                f"{rp.skip_reason}",
            )
            self.assertIsNotNone(rp.rubric)
            self.assertEqual(rp.rubric.id, "anvil-pub-v2")
            self.assertEqual(rp.rubric.total, 44)
            self.assertEqual(rp.rubric.advance_threshold, 35)
            self.assertIsNotNone(rp.stamp_meta)
            self.assertEqual(rp.stamp_meta.rubric_id, "anvil-pub-v2")
            self.assertEqual(rp.stamp_meta.rubric_total, 44)
            self.assertEqual(rp.stamp_meta.advance_threshold, 35)


class TestEssay44AutoInference(unittest.TestCase):
    """End-to-end: an essay review with `rubric_total: 44` but no
    `rubric_id` should resolve to `anvil-essay-v1` without
    `--legacy-rubric` (issue #482, mirroring the #366
    TestPub44AutoInference pattern).

    Before #482, this skipped twice over: no `("essay", 44)` row in
    KNOWN_RUBRICS AND no `essay` row in detect's BRIEF
    artifact_type → skill map (essay bodies echo the slug, so the
    fixed-body-filename rule cannot fire — the BRIEF route is the
    only inference path).
    """

    def test_essay_44_heuristic_inference_resolves_to_v1(self) -> None:
        with TemporaryDirectory() as td:
            project = build_essay_44_unstamped(Path(td))
            inv = inventory_tree(project)
            self.assertEqual(len(inv.reviews), 1)
            self.assertEqual(inv.reviews[0].inferred_skill, "essay")
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            rp = p.reviews[0]
            self.assertFalse(
                rp.skipped,
                f"Expected stamping plan; got skip with reason: "
                f"{rp.skip_reason}",
            )
            self.assertIsNotNone(rp.rubric)
            self.assertEqual(rp.rubric.id, "anvil-essay-v1")
            self.assertEqual(rp.rubric.total, 44)
            self.assertEqual(rp.rubric.advance_threshold, 35)
            self.assertIsNotNone(rp.stamp_meta)
            self.assertEqual(rp.stamp_meta.rubric_id, "anvil-essay-v1")
            self.assertEqual(rp.stamp_meta.rubric_total, 44)
            self.assertEqual(rp.stamp_meta.advance_threshold, 35)


class TestDatasheet44AutoInference(unittest.TestCase):
    """End-to-end: a datasheet review with `rubric_total: 44` but no
    `rubric_id` should resolve to `anvil-datasheet-v1` without
    `--legacy-rubric` (issue #486, mirroring the #482
    TestEssay44AutoInference pattern).

    This is the path #484 left stranded: the `("datasheet", 44)`
    KNOWN_RUBRICS row and detect's `datasheet` BRIEF map row both
    existed, but `artifact_type: datasheet` was rejected by strict
    BRIEF validation because the type was absent from
    `REGISTERED_ARTIFACT_TYPES`. #486 registers it, giving rule-1
    BRIEF-route inference a validated carrier. The body is a fixed-name
    `datasheet.tex` that is intentionally NOT in
    `_BODY_FILENAME_TO_SKILL`, so rule-2 stays inert — the BRIEF route
    is the only path (parity with provisional's `spec.tex`).
    """

    def test_datasheet_44_heuristic_inference_resolves_to_v1(self) -> None:
        with TemporaryDirectory() as td:
            project = build_datasheet_44_unstamped(Path(td))
            inv = inventory_tree(project)
            self.assertEqual(len(inv.reviews), 1)
            self.assertEqual(inv.reviews[0].inferred_skill, "datasheet")
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            rp = p.reviews[0]
            self.assertFalse(
                rp.skipped,
                f"Expected stamping plan; got skip with reason: "
                f"{rp.skip_reason}",
            )
            self.assertIsNotNone(rp.rubric)
            self.assertEqual(rp.rubric.id, "anvil-datasheet-v1")
            self.assertEqual(rp.rubric.total, 44)
            self.assertEqual(rp.rubric.advance_threshold, 39)
            self.assertIsNotNone(rp.stamp_meta)
            self.assertEqual(rp.stamp_meta.rubric_id, "anvil-datasheet-v1")
            self.assertEqual(rp.stamp_meta.rubric_total, 44)
            self.assertEqual(rp.stamp_meta.advance_threshold, 39)


# ---------------------------------------------------------------------------
# Issue #482 — catalog drift guard
# ---------------------------------------------------------------------------


_RUBRIC_ID_RE = re.compile(r"anvil-[a-z0-9-]+-v[0-9]+(?:-legacy-[0-9]+)?")


class TestCatalogDriftGuard(unittest.TestCase):
    """KNOWN_RUBRICS must not silently fall behind the shipped skills.

    This is the SECOND catalog-lag occurrence: #366 backfilled the
    post-#357 /44 rows; datasheet (#421), ip-uspto-provisional (#444),
    and essay (#477) then all shipped without catalog updates (#482).
    The guard: every skill that ships a ``rubric.md`` (the
    artifact-class marker — bridge/utility skills ship none) must
    (a) resolve in CURRENT_RUBRIC_BY_SKILL and (b) have its review
    command's stamped rubric_id literal match the catalog's current id.

    Why the review command, not rubric.md: four mature skills'
    rubric.md files carry no id literal at all, and pub/report
    rubric.md's only id literals are the vision-critic rubric ids
    (``anvil-pub-vision-v1`` …), which are deliberately NOT in this
    catalog. ``commands/<skill>-review.md`` is the authoritative
    stamping site (per the #366 comment in plan.py).
    """

    # _HERE = anvil/skills/rubric-rebackport/tests/ → parents[1] is
    # anvil/skills/.
    SKILLS_DIR = _HERE.parents[1]

    @classmethod
    def _artifact_class_skills(cls) -> list:
        return sorted(
            p.parent.name for p in cls.SKILLS_DIR.glob("*/rubric.md")
        )

    def _stamped_ids(self, skill: str) -> set:
        """Extract id literals appearing in rubric_id context in the
        skill's review command."""
        review_cmd = (
            self.SKILLS_DIR / skill / "commands" / f"{skill}-review.md"
        )
        self.assertTrue(
            review_cmd.is_file(),
            f"`{skill}` ships rubric.md but no commands/{skill}-review.md "
            "— extend the drift guard for its layout.",
        )
        text = review_cmd.read_text(encoding="utf-8")
        return {
            m.group(0)
            for m in _RUBRIC_ID_RE.finditer(text)
            if "rubric_id" in text[max(0, m.start() - 60) : m.start()]
        }

    def test_glob_sees_the_artifact_class_skills(self) -> None:
        """Sanity: the rubric.md glob actually finds skills (layout pin)."""
        skills = self._artifact_class_skills()
        self.assertGreaterEqual(
            len(skills),
            11,
            f"rubric.md glob under {self.SKILLS_DIR} found only {skills} — "
            "repo layout changed; re-anchor the drift guard.",
        )
        for non_artifact in (
            "project-migrate",
            "rubric-rebackport",
            "project-share",
            "project-scout",
        ):
            self.assertNotIn(non_artifact, skills)

    def test_every_rubric_shipping_skill_has_a_current_catalog_row(
        self,
    ) -> None:
        for skill in self._artifact_class_skills():
            self.assertIn(
                skill,
                CURRENT_RUBRIC_BY_SKILL,
                f"`{skill}` ships a rubric.md but has no "
                "CURRENT_RUBRIC_BY_SKILL row — the KNOWN_RUBRICS catalog "
                "fell behind again (third occurrence; see #366, #482). "
                "Add the (skill, total) row(s) to plan.py and the "
                "SKILL.md inference table.",
            )

    def test_stamped_rubric_id_matches_catalog_current_id(self) -> None:
        """The review command's stamped id literal must be the catalog's
        current id — catches transcription drift (e.g., the
        `anvil-ip-provisional-v1` vs `anvil-ip-uspto-provisional-v1`
        trap) in either direction."""
        for skill in self._artifact_class_skills():
            current = CURRENT_RUBRIC_BY_SKILL.get(skill)
            if current is None:
                continue  # covered by the test above
            stamped = self._stamped_ids(skill)
            self.assertIn(
                current.id,
                stamped,
                f"`{skill}`: catalog current rubric_id `{current.id}` does "
                f"not appear as a stamped rubric_id literal in "
                f"commands/{skill}-review.md (found: {sorted(stamped)}). "
                "Either the catalog row or the review command is wrong.",
            )
            self.assertIsNotNone(lookup_rubric_by_id(current.id))


class TestHeuristicMiss(unittest.TestCase):
    def test_no_legacy_rubric_no_total_skips_review(self) -> None:
        """If neither --legacy-rubric nor _meta.rubric_total is set, skip."""
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            meta_path = inv.reviews[0].meta_path
            data = json.loads(meta_path.read_text())
            data.pop("rubric_total", None)
            meta_path.write_text(json.dumps(data, indent=2) + "\n")
            inv = inventory_tree(project)
            p = build_plan(inv, mode=Mode.STAMP_ONLY)
            rp = p.reviews[0]
            self.assertTrue(rp.skipped)
            self.assertIn("rubric_total", rp.skip_reason)


# ---------------------------------------------------------------------------
# Issue #374 — `--skill` as filter / force-set hybrid
# ---------------------------------------------------------------------------


class TestSkillFilterForceSetSemantics(unittest.TestCase):
    """Pin the post-#374 `--skill=<X>` behavior matrix:

    1. inferred_skill is None    AND --skill=<X> -> FORCE (stamp under X)
    2. inferred_skill != X       AND --skill=<X> -> FILTER (skip)
    3. inferred_skill == X       AND --skill=<X> -> NORMAL (stamp)

    Per Option B in the curator enrichment: the prior-release behavior
    was pure filter for all three cases, which left the canary's deck
    threads (with `aldus/aldus.4/deck.md`, no BRIEF) skipped with
    `outside --skill=deck scope (inferred skill: None)` even though
    the operator's assertion carried enough information to stamp.
    """

    def test_skill_filter_forces_when_inference_returns_none(self) -> None:
        """Case 1: inference returned None, --skill=<X> forces the stamp."""
        with TemporaryDirectory() as td:
            project = build_unconventional_body_filename_thread(Path(td))
            inv = inventory_tree(project)
            self.assertEqual(len(inv.reviews), 1)
            self.assertIsNone(
                inv.reviews[0].inferred_skill,
                "fixture must produce inferred_skill=None for the "
                "force-set test to be meaningful (rule 2 inference "
                "table should NOT contain `body.md`)",
            )
            p = build_plan(
                inv,
                mode=Mode.STAMP_ONLY,
                skill_filter="deck",
                legacy_rubric="anvil-deck-v1",
            )
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertFalse(
                rp.skipped,
                f"force-set should stamp, not skip; reason: {rp.skip_reason}",
            )
            self.assertEqual(rp.skill, "deck")
            self.assertIsNotNone(rp.rubric)
            self.assertEqual(rp.rubric.id, "anvil-deck-v1")
            self.assertIsNotNone(rp.stamp_meta)
            # Operator-visible disclosure of the override in notes.
            self.assertTrue(
                any("forced" in n.lower() for n in rp.notes),
                f"expected `forced` note; got notes: {rp.notes}",
            )

    def test_skill_filter_still_filters_when_inference_disagrees(self) -> None:
        """Case 2: inferred_skill is set AND disagrees -> skip with `outside`."""
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))  # memo fixture
            inv = inventory_tree(project)
            self.assertEqual(inv.reviews[0].inferred_skill, "memo")
            p = build_plan(
                inv,
                mode=Mode.STAMP_ONLY,
                skill_filter="deck",
            )
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertTrue(rp.skipped)
            self.assertIn("outside", rp.skip_reason)
            self.assertIn("deck", rp.skip_reason)

    def test_skill_filter_no_effect_when_inference_agrees(self) -> None:
        """Case 3: inferred_skill == --skill=<X> -> normal stamp, no notes diff."""
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))  # memo fixture
            inv = inventory_tree(project)
            unfiltered = build_plan(inv, mode=Mode.STAMP_ONLY)
            inv2 = inventory_tree(project)
            filtered = build_plan(
                inv2, mode=Mode.STAMP_ONLY, skill_filter="memo"
            )
            self.assertEqual(len(filtered.reviews), 1)
            rp_u = unfiltered.reviews[0]
            rp_f = filtered.reviews[0]
            self.assertFalse(rp_f.skipped)
            self.assertEqual(rp_f.skill, rp_u.skill)
            self.assertEqual(rp_f.rubric.id, rp_u.rubric.id)
            # The agree-case must NOT add a "forced" note; that note is
            # reserved for the force-set-on-None case.
            self.assertFalse(
                any("forced" in n.lower() for n in rp_f.notes),
                f"agree-case should not emit a `forced` note; got: {rp_f.notes}",
            )


class TestDeckBodyFilenameInference(unittest.TestCase):
    """Pin the #374 table extension: ``deck.md`` / ``slides.md`` /
    ``ip-uspto.md`` are now in ``_BODY_FILENAME_TO_SKILL`` so rule 2
    of ``_infer_skill`` resolves them without needing a BRIEF.
    """

    def test_deck_thread_no_brief_infers_via_body_filename(self) -> None:
        """Canary repro: deck thread with `deck.md`, no BRIEF -> infers `deck`."""
        with TemporaryDirectory() as td:
            project = build_deck_thread_no_brief(Path(td))
            inv = inventory_tree(project)
            self.assertEqual(len(inv.reviews), 1)
            self.assertEqual(inv.reviews[0].inferred_skill, "deck")
            self.assertEqual(inv.reviews[0].skill_source, "body-filename")

    def test_table_extended_with_deck_slides_ip_uspto(self) -> None:
        """Pin that the three #374 entries are in the table."""
        self.assertEqual(detect._BODY_FILENAME_TO_SKILL["deck.md"], "deck")
        self.assertEqual(
            detect._BODY_FILENAME_TO_SKILL["slides.md"], "slides"
        )
        self.assertEqual(
            detect._BODY_FILENAME_TO_SKILL["ip-uspto.md"], "ip-uspto"
        )


class TestCanaryDeckRebackportE2E(unittest.TestCase):
    """End-to-end smoke test for the canary's #374 reproducer.

    The canary's original on-disk shape was a deck thread with
    ``aldus/aldus.4/deck.md`` and an unstamped ``aldus.4.review/
    _meta.json``. Pre-fix, this skipped with ``outside --skill=deck
    scope (inferred skill: None)``. Post-fix (either path):

    1. Table extension: rule 2 of ``_infer_skill`` resolves to ``deck``,
       and the planner stamps normally without needing the force-set.
    2. Force-set: if the body filename were also non-canonical (covered
       by ``test_skill_filter_forces_when_inference_returns_none``),
       the planner still stamps via the operator assertion.

    This test pins the first path (rule 2 succeeds) for the exact
    canary on-disk shape.
    """

    def test_canary_deck_thread_with_skill_flag_stamps_to_deck_v1(self) -> None:
        with TemporaryDirectory() as td:
            project = build_deck_thread_no_brief(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.STAMP_ONLY,
                skill_filter="deck",
                legacy_rubric="anvil-deck-v1",
            )
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertFalse(
                rp.skipped,
                f"canary repro should stamp; got skip: {rp.skip_reason}",
            )
            self.assertEqual(rp.skill, "deck")
            self.assertEqual(rp.rubric.id, "anvil-deck-v1")
            self.assertEqual(rp.rubric.total, 40)
            self.assertEqual(rp.rubric.advance_threshold, 35)
            self.assertIsNotNone(rp.stamp_meta)

    def test_canary_deck_thread_without_skill_flag_also_stamps(self) -> None:
        """Without --skill, the table-extension fix lets rule 2 still resolve."""
        with TemporaryDirectory() as td:
            project = build_deck_thread_no_brief(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.STAMP_ONLY,
                legacy_rubric="anvil-deck-v1",
            )
            self.assertEqual(len(p.reviews), 1)
            rp = p.reviews[0]
            self.assertFalse(rp.skipped)
            self.assertEqual(rp.skill, "deck")


if __name__ == "__main__":
    unittest.main()
