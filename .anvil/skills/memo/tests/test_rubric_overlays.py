"""Tests for ``anvil/skills/memo/lib/rubric_overlays.py``.

Covers:

- Each of the 7 shipped overlay JSON files loads cleanly via
  :func:`load_overlay` and round-trips its declared artifact_type
  (five #286 seeds plus the #394 canary genres challenge-memo /
  strategy-memo).
- The #394 consumer overlay tier
  (``<consumer>/.anvil/skills/memo/rubric_overlays/<type>.json``):
  consumer-declared types resolve end-to-end, consumer wins over
  shipped on collision, malformed consumer JSONs raise
  ``OverlayLoadError``, and the #386 skill-identity guard is unaffected.
- The investment-memo overlay is identity (all-zero adjustments,
  empty calibration prose).
- :func:`select_overlay_for_thread` resolves correctly for both layouts
  (no project BRIEF → None; project-brief → matching overlay; unlisted slug → None).
- :class:`OverlayLoadError` fires on:
  - missing file
  - invalid JSON
  - schema violation (wrong artifact_type field, unknown dim key, extra
    top-level field via ``extra="forbid"``)
  - filename ↔ declared artifact_type mismatch
- Doc cross-references in SKILL.md, rubric.md, and memo-review.md mention
  the new overlay surface.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

# Test module path setup mirrors the project_discovery / project_brief
# test files in this directory.
_LIB_DIR = Path(__file__).resolve().parent.parent / "lib"
if str(_LIB_DIR) not in sys.path:
    sys.path.insert(0, str(_LIB_DIR))

from project_brief import (  # noqa: E402
    MEMO_ARTIFACT_TYPES,
    ArtifactType,
)
from rubric_overlays import (  # noqa: E402
    OVERLAYS_DIR,
    OverlayLoadError,
    RubricOverlay,
    load_overlay,
    select_overlay_for_thread,
)


class TestRegistryShape(unittest.TestCase):
    """The shipped overlay registry covers every MEMO artifact type.

    Issue #386 grew the shared enum with skill-identity values (deck /
    slides / proposal) that select NO memo overlay — the registry is
    scoped to ``MEMO_ARTIFACT_TYPES``, not the full enum.
    """

    def test_one_overlay_file_per_memo_artifact_type(self) -> None:
        shipped = {p.stem for p in OVERLAYS_DIR.glob("*.json")}
        memo_scoped = {t.value for t in MEMO_ARTIFACT_TYPES}
        self.assertEqual(
            shipped,
            memo_scoped,
            "Every memo-scoped ArtifactType must have a shipped overlay "
            "JSON; and the registry must NOT contain orphans — in "
            "particular no identity deck/slides/proposal overlays "
            "(those would silently mis-score non-memo artifacts).",
        )

    def test_skill_identity_types_have_no_overlay_files(self) -> None:
        for at in (ArtifactType.DECK, ArtifactType.SLIDES, ArtifactType.PROPOSAL):
            with self.subTest(artifact_type=at.value):
                self.assertNotIn(at, MEMO_ARTIFACT_TYPES)
                self.assertFalse(
                    (OVERLAYS_DIR / f"{at.value}.json").exists(),
                    f"{at.value}.json must NOT ship in memo's overlay "
                    "registry (issue #386).",
                )

    def test_all_shipped_overlays_load_without_error(self) -> None:
        for at in sorted(MEMO_ARTIFACT_TYPES, key=lambda t: t.value):
            with self.subTest(artifact_type=at.value):
                overlay = load_overlay(at)
                self.assertIsInstance(overlay, RubricOverlay)
                self.assertEqual(overlay.artifact_type, at)


class TestInvestmentMemoIdentity(unittest.TestCase):
    """The investment-memo overlay is the canonical identity overlay."""

    def test_investment_memo_is_identity(self) -> None:
        overlay = load_overlay(ArtifactType.INVESTMENT_MEMO)
        self.assertTrue(
            overlay.is_identity(),
            "investment-memo overlay must be identity (zero adjustments, "
            "no calibration prose) to preserve byte-identical v0 behavior "
            "for threads with artifact_type=investment-memo.",
        )

    def test_non_investment_memo_overlays_are_not_identity(self) -> None:
        for at in sorted(MEMO_ARTIFACT_TYPES, key=lambda t: t.value):
            if at == ArtifactType.INVESTMENT_MEMO:
                continue
            with self.subTest(artifact_type=at.value):
                overlay = load_overlay(at)
                self.assertFalse(
                    overlay.is_identity(),
                    f"{at.value} overlay should make at least one change "
                    "(either a weight adjustment or calibration prose) "
                    "— an identity overlay for a non-investment-memo type "
                    "is almost certainly a bug.",
                )


class TestWeightAdjustments(unittest.TestCase):
    """Per-dim weight adjustments stay within sensible bounds."""

    BASE_WEIGHTS = {
        "dim_1": 5,
        "dim_2": 6,
        "dim_3": 6,
        "dim_4": 6,
        "dim_5": 4,
        "dim_6": 5,
        "dim_7": 4,
        "dim_8": 4,
        "dim_9": 4,
    }

    def test_no_overlay_drives_any_dim_negative(self) -> None:
        for at in sorted(MEMO_ARTIFACT_TYPES, key=lambda t: t.value):
            overlay = load_overlay(at)
            for dim, delta in overlay.weight_adjustments.items():
                base = self.BASE_WEIGHTS[dim]
                effective = base + delta
                with self.subTest(artifact_type=at.value, dim=dim):
                    self.assertGreaterEqual(
                        effective,
                        0,
                        f"{at.value}/{dim}: base={base} + delta={delta} = "
                        f"{effective}; overlays must not drive a dim below 0.",
                    )

    def test_dim_keys_are_dim_1_through_dim_9(self) -> None:
        valid = {f"dim_{n}" for n in range(1, 10)}
        for at in sorted(MEMO_ARTIFACT_TYPES, key=lambda t: t.value):
            overlay = load_overlay(at)
            for dim in overlay.weight_adjustments:
                self.assertIn(dim, valid)
            for dim in overlay.calibration_prose:
                self.assertIn(dim, valid)


class TestSelectOverlayForThread(unittest.TestCase):
    """End-to-end selection from a thread dir under a project BRIEF."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write_project(
        self,
        doc_slugs_and_types: list[tuple[str, str]],
        project_name: str = "test-project",
    ) -> Path:
        """Create a project with a BRIEF.md listing the given (slug, type) pairs.

        Each thread directory is materialized empty. ``project_name``
        lets a single test build several projects in one tmp dir.
        """
        project_root = self.tmp_path / project_name
        project_root.mkdir()

        documents_block = "\n".join(
            f"  - slug: {slug}\n    artifact_type: {atype}"
            for slug, atype in doc_slugs_and_types
        )
        brief = (
            "---\n"
            f"project: test-project\n"
            f"audience: [team]\n"
            f"hard_rules: []\n"
            f"documents:\n{documents_block}\n"
            "---\n"
            "\n"
            "Project brief body.\n"
        )
        (project_root / "BRIEF.md").write_text(brief, encoding="utf-8")
        for slug, _ in doc_slugs_and_types:
            (project_root / slug).mkdir()
        return project_root

    def test_position_paper_thread_resolves_to_position_paper_overlay(self) -> None:
        project_root = self._write_project([("latency-wall", "position-paper")])
        thread_dir = project_root / "latency-wall"
        overlay = select_overlay_for_thread(thread_dir)
        self.assertIsNotNone(overlay)
        self.assertEqual(overlay.artifact_type, ArtifactType.POSITION_PAPER)

    def test_investment_memo_thread_resolves_to_identity_overlay(self) -> None:
        project_root = self._write_project(
            [("investment-memo", "investment-memo")]
        )
        thread_dir = project_root / "investment-memo"
        overlay = select_overlay_for_thread(thread_dir)
        self.assertIsNotNone(overlay)
        self.assertTrue(overlay.is_identity())

    def test_thread_without_project_brief_resolves_to_none(self) -> None:
        # A thread with no project BRIEF on the walk-upward path → no
        # overlay selected. Under #295 every thread is expected to live
        # under a project root; a stray thread that does not satisfy
        # that contract returns None here (and is non-discoverable per
        # project_discovery.discover_thread_root).
        thread_root = self.tmp_path / "standalone-thread"
        thread_root.mkdir()
        (thread_root / "standalone-thread.1").mkdir()
        overlay = select_overlay_for_thread(thread_root)
        self.assertIsNone(overlay)

    def test_unlisted_thread_under_project_brief_resolves_to_none(self) -> None:
        # The thread is on disk under a project-BRIEF root but its slug
        # is not in the BRIEF's documents: list.
        project_root = self._write_project([("listed-thread", "investment-memo")])
        unlisted = project_root / "unlisted-thread"
        unlisted.mkdir()
        overlay = select_overlay_for_thread(unlisted)
        self.assertIsNone(overlay)

    def test_explicit_project_dir_override(self) -> None:
        project_root = self._write_project(
            [("vision-doc", "vision-document")]
        )
        thread_dir = project_root / "vision-doc"
        overlay = select_overlay_for_thread(thread_dir, project_dir=project_root)
        self.assertIsNotNone(overlay)
        self.assertEqual(overlay.artifact_type, ArtifactType.VISION_DOCUMENT)

    def test_non_memo_artifact_type_raises_skill_mismatch(self) -> None:
        """Issue #386: a memo command pointed at a deck/slides/proposal-
        typed thread fails LOUDLY with a self-explaining skill-mismatch
        error — not a confusing 'No overlay file found' message, and
        never a silent identity overlay."""
        for value in ("deck", "slides", "proposal"):
            with self.subTest(artifact_type=value):
                slug = f"{value}-thread"
                project_root = self._write_project(
                    [(slug, value)], project_name=f"project-{value}"
                )
                thread_dir = project_root / slug
                with self.assertRaises(OverlayLoadError) as ctx:
                    select_overlay_for_thread(
                        thread_dir, project_dir=project_root
                    )
                msg = str(ctx.exception)
                # Names the artifact_type and the slug.
                self.assertIn(value, msg)
                self.assertIn(slug, msg)
                # States the memo-only constraint.
                self.assertIn("memo artifact types", msg)
                # Not the missing-overlay-file diagnostic.
                self.assertNotIn("No overlay file found", msg)

    def test_memo_types_unaffected_by_subset_guard(self) -> None:
        """Every memo-scoped type still resolves through the guard to
        its overlay — the guard keeps existing memo paths byte-identical."""
        for at in sorted(MEMO_ARTIFACT_TYPES, key=lambda t: t.value):
            with self.subTest(artifact_type=at.value):
                slug = f"{at.value}-thread"
                project_root = self._write_project(
                    [(slug, at.value)], project_name=f"project-{at.value}"
                )
                overlay = select_overlay_for_thread(
                    project_root / slug, project_dir=project_root
                )
                self.assertIsNotNone(overlay)
                self.assertEqual(overlay.artifact_type, at)


class TestConsumerOverlayTier(unittest.TestCase):
    """Issue #394: the consumer overlay registry at
    ``<consumer>/.anvil/skills/memo/rubric_overlays/<type>.json``.

    A consumer declares new memo genres (and recalibrates shipped ones)
    without a framework release. Resolution is two-tier — consumer
    first, shipped second.
    """

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)
        self.consumer_dir = (
            self.tmp_path / ".anvil" / "skills" / "memo" / "rubric_overlays"
        )
        self.consumer_dir.mkdir(parents=True)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write_consumer_overlay(
        self, type_name: str, payload: dict | None = None
    ) -> Path:
        path = self.consumer_dir / f"{type_name}.json"
        if payload is None:
            payload = {
                "artifact_type": type_name,
                "description": f"Consumer-declared {type_name} overlay.",
                "weight_adjustments": {"dim_6": -2},
                "calibration_prose": {"dim_1": f"{type_name} calibration."},
            }
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def _write_project(self, slug: str, atype: str) -> Path:
        project_root = self.tmp_path / "project"
        project_root.mkdir(exist_ok=True)
        brief = (
            "---\n"
            "project: test-project\n"
            "documents:\n"
            f"  - slug: {slug}\n    artifact_type: {atype}\n"
            "---\n\nProject brief body.\n"
        )
        (project_root / "BRIEF.md").write_text(brief, encoding="utf-8")
        (project_root / slug).mkdir(exist_ok=True)
        return project_root

    def test_consumer_declared_type_resolves_end_to_end(self) -> None:
        """A BRIEF declaring an unregistered consumer-overlay-backed type
        parses cleanly AND select_overlay_for_thread applies the overlay
        deterministically (acceptance criterion 2 of #394)."""
        self._write_consumer_overlay("field-note")
        project_root = self._write_project("notes", "field-note")
        overlay = select_overlay_for_thread(
            project_root / "notes", project_dir=project_root
        )
        self.assertIsNotNone(overlay)
        self.assertEqual(overlay.artifact_type, "field-note")
        self.assertEqual(overlay.weight_adjustments, {"dim_6": -2})
        self.assertEqual(
            overlay.calibration_prose, {"dim_1": "field-note calibration."}
        )

    def test_consumer_tier_wins_over_shipped(self) -> None:
        """A consumer recalibration of a SHIPPED type (their own
        position-paper.json) wins over the shipped overlay."""
        self._write_consumer_overlay(
            "position-paper",
            {
                "artifact_type": "position-paper",
                "description": "Consumer recalibration of position-paper.",
                "weight_adjustments": {"dim_1": -1},
                "calibration_prose": {},
            },
        )
        project_root = self._write_project("latency-wall", "position-paper")
        overlay = select_overlay_for_thread(
            project_root / "latency-wall", project_dir=project_root
        )
        self.assertIsNotNone(overlay)
        self.assertEqual(overlay.weight_adjustments, {"dim_1": -1})
        self.assertIn("Consumer recalibration", overlay.description)

    def test_load_overlay_explicit_consumer_dir(self) -> None:
        """load_overlay resolves a consumer type when handed the consumer
        overlay dir directly (no marker walk needed)."""
        self._write_consumer_overlay("field-note")
        overlay = load_overlay(
            "field-note", consumer_overlays_dir=self.consumer_dir
        )
        self.assertEqual(overlay.artifact_type, "field-note")

    def test_malformed_consumer_json_raises_with_path(self) -> None:
        path = self.consumer_dir / "field-note.json"
        path.write_text("{ not valid json", encoding="utf-8")
        with self.assertRaises(OverlayLoadError) as ctx:
            load_overlay("field-note", consumer_overlays_dir=self.consumer_dir)
        self.assertIn("invalid JSON", str(ctx.exception))
        self.assertIn(str(path), str(ctx.exception))

    def test_unknown_dim_key_in_consumer_overlay_raises(self) -> None:
        self._write_consumer_overlay(
            "field-note",
            {
                "artifact_type": "field-note",
                "description": "Bad dim key.",
                "weight_adjustments": {"dim_42": -1},
                "calibration_prose": {},
            },
        )
        with self.assertRaises(OverlayLoadError) as ctx:
            load_overlay("field-note", consumer_overlays_dir=self.consumer_dir)
        self.assertIn("dim_42", str(ctx.exception))

    def test_consumer_filename_type_mismatch_raises(self) -> None:
        self._write_consumer_overlay(
            "field-note",
            {
                "artifact_type": "lab-journal",
                "description": "Filename mismatch.",
                "weight_adjustments": {},
                "calibration_prose": {},
            },
        )
        with self.assertRaises(OverlayLoadError) as ctx:
            load_overlay("field-note", consumer_overlays_dir=self.consumer_dir)
        self.assertIn("filename mismatch", str(ctx.exception))

    def test_overlay_deleted_after_brief_written_fails_loudly(self) -> None:
        """An overlay JSON deleted after the BRIEF was written must still
        fail loudly at review time — the BRIEF parse rejects the
        now-unbacked type with the available-set error."""
        overlay_path = self._write_consumer_overlay("field-note")
        project_root = self._write_project("notes", "field-note")
        overlay_path.unlink()
        with self.assertRaises(ValueError) as ctx:
            select_overlay_for_thread(
                project_root / "notes", project_dir=project_root
            )
        msg = str(ctx.exception)
        self.assertIn("field-note", msg)
        self.assertIn("rubric_overlays", msg)

    def test_missing_overlay_defense_in_depth_in_load_overlay(self) -> None:
        """Defense in depth: load_overlay itself fails loudly with the
        available-set error for a type with no overlay in either tier."""
        self._write_consumer_overlay("field-note")
        with self.assertRaises(OverlayLoadError) as ctx:
            load_overlay(
                "lab-journal", consumer_overlays_dir=self.consumer_dir
            )
        msg = str(ctx.exception)
        self.assertIn("No overlay file found", msg)
        self.assertIn("field-note", msg)  # consumer set enumerated
        self.assertIn("position-paper", msg)  # shipped set enumerated

    def test_skill_identity_guard_unaffected_by_consumer_tier(self) -> None:
        """The #386 guard still fires for every skill-identity value
        even when a consumer overlay registry exists — the guard is
        keyed on the explicit SKILL_IDENTITY_ARTIFACT_TYPES set (`paper`
        joined under #408 as `pub`, renamed to `paper` under #694,
        `report` under #432, `ip-uspto` /
        `ip-uspto-provisional` under #440, `essay` under #460,
        `datasheet` under #486 — memo overlay dispatch must
        fail loudly on all of them)."""
        self._write_consumer_overlay("field-note")
        for value in (
            "deck",
            "slides",
            "proposal",
            "paper",
            "report",
            "ip-uspto",
            "ip-uspto-provisional",
            "essay",
            "datasheet",
        ):
            with self.subTest(artifact_type=value):
                slug = f"{value}-thread"
                project_root = self._write_project(slug, value)
                with self.assertRaises(OverlayLoadError) as ctx:
                    select_overlay_for_thread(
                        project_root / slug, project_dir=project_root
                    )
                msg = str(ctx.exception)
                self.assertIn(value, msg)
                self.assertIn("memo artifact types", msg)
                self.assertNotIn("No overlay file found", msg)


class TestCanaryGenreOverlays(unittest.TestCase):
    """Issue #394 part 1: the shipped challenge-memo / strategy-memo
    overlays load, are non-identity, and calibrate dims 1/5/6."""

    def test_challenge_memo_overlay_calibrates_dims_1_5_6(self) -> None:
        overlay = load_overlay(ArtifactType.CHALLENGE_MEMO)
        self.assertFalse(overlay.is_identity())
        for dim in ("dim_1", "dim_5", "dim_6"):
            self.assertIn(dim, overlay.calibration_prose)
        # Verdict-on-the-test framing, not invest/pass.
        self.assertIn("verdict", overlay.calibration_prose["dim_1"])

    def test_strategy_memo_overlay_calibrates_dims_1_5_6(self) -> None:
        overlay = load_overlay(ArtifactType.STRATEGY_MEMO)
        self.assertFalse(overlay.is_identity())
        for dim in ("dim_1", "dim_5", "dim_6"):
            self.assertIn(dim, overlay.calibration_prose)
        # Actionability-of-the-play framing.
        self.assertIn("actionability", overlay.calibration_prose["dim_1"])


class TestLoadOverlayErrors(unittest.TestCase):
    """OverlayLoadError covers every load-time failure mode."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _load_from_path(self, overlay_path: Path) -> RubricOverlay:
        """Bypass OVERLAYS_DIR by writing into the real dir under a test name
        and cleaning up. We test the real load path here.
        """
        raise NotImplementedError  # placeholder — tests below use real OVERLAYS_DIR

    def test_missing_overlay_file_raises(self) -> None:
        # Construct a fake ArtifactType-like by patching OVERLAYS_DIR via
        # a monkeypatched helper would be heavy; we trust the production
        # path that exists (one file per registered type). To simulate
        # missing-file, temporarily move one aside.
        target = OVERLAYS_DIR / "position-paper.json"
        backup = OVERLAYS_DIR / "position-paper.json.bak"
        target.rename(backup)
        try:
            with self.assertRaises(OverlayLoadError) as ctx:
                load_overlay(ArtifactType.POSITION_PAPER)
            self.assertIn("No overlay file found", str(ctx.exception))
            self.assertIn("position-paper", str(ctx.exception))
        finally:
            backup.rename(target)

    def test_invalid_json_raises(self) -> None:
        target = OVERLAYS_DIR / "tactical-plan.json"
        original = target.read_text(encoding="utf-8")
        target.write_text("{ not valid json", encoding="utf-8")
        try:
            with self.assertRaises(OverlayLoadError) as ctx:
                load_overlay(ArtifactType.TACTICAL_PLAN)
            self.assertIn("invalid JSON", str(ctx.exception))
        finally:
            target.write_text(original, encoding="utf-8")

    def test_unknown_dim_key_in_weight_adjustments_raises(self) -> None:
        target = OVERLAYS_DIR / "vision-document.json"
        original = target.read_text(encoding="utf-8")
        # Inject an unknown dim key.
        bad = json.loads(original)
        bad["weight_adjustments"]["dim_99"] = -1
        target.write_text(json.dumps(bad), encoding="utf-8")
        try:
            with self.assertRaises(OverlayLoadError) as ctx:
                load_overlay(ArtifactType.VISION_DOCUMENT)
            self.assertIn("dim_99", str(ctx.exception))
            self.assertIn("weight_adjustments", str(ctx.exception))
        finally:
            target.write_text(original, encoding="utf-8")

    def test_filename_mismatch_raises(self) -> None:
        # Write an overlay JSON whose declared artifact_type doesn't match
        # the filename we ask for. The mismatch check fires after Pydantic
        # validates the JSON content.
        target = OVERLAYS_DIR / "descriptive-thesis.json"
        original = target.read_text(encoding="utf-8")
        bad = json.loads(original)
        bad["artifact_type"] = "position-paper"
        target.write_text(json.dumps(bad), encoding="utf-8")
        try:
            with self.assertRaises(OverlayLoadError) as ctx:
                load_overlay(ArtifactType.DESCRIPTIVE_THESIS)
            self.assertIn("filename mismatch", str(ctx.exception))
        finally:
            target.write_text(original, encoding="utf-8")

    def test_extra_top_level_field_raises(self) -> None:
        target = OVERLAYS_DIR / "investment-memo.json"
        original = target.read_text(encoding="utf-8")
        bad = json.loads(original)
        bad["unexpected_field"] = 42
        target.write_text(json.dumps(bad), encoding="utf-8")
        try:
            with self.assertRaises(OverlayLoadError) as ctx:
                load_overlay(ArtifactType.INVESTMENT_MEMO)
            self.assertIn("schema error", str(ctx.exception))
        finally:
            target.write_text(original, encoding="utf-8")


class TestPathSeparatorGuard(unittest.TestCase):
    """Issue #403: load_overlay rejects path-shaped artifact_type values
    before any path construction (defense in depth post-#394, which
    relaxed artifact_type to arbitrary str)."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)
        self.consumer_dir = self.tmp_path / "overlays"
        self.consumer_dir.mkdir()

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_load_overlay_rejects_path_separator_artifact_type(self) -> None:
        # Plant a file at the traversal target to prove the guard fires
        # BEFORE path resolution — without the guard, "../evil" would
        # resolve to this file and load successfully.
        (self.tmp_path / "evil.json").write_text(
            json.dumps(
                {
                    "artifact_type": "../evil",
                    "description": "Traversal payload.",
                    "weight_adjustments": {},
                    "calibration_prose": {},
                }
            ),
            encoding="utf-8",
        )
        for bad in ("../evil", "a/b", "a\\b", ".", ".."):
            with self.subTest(artifact_type=bad):
                with self.assertRaises(OverlayLoadError) as ctx:
                    load_overlay(bad, consumer_overlays_dir=self.consumer_dir)
                self.assertIn("not a valid overlay slug", str(ctx.exception))

    def test_load_overlay_rejects_empty_artifact_type(self) -> None:
        # Without the guard, "" would probe a hidden ".json" file.
        with self.assertRaises(OverlayLoadError) as ctx:
            load_overlay("", consumer_overlays_dir=self.consumer_dir)
        self.assertIn("not a valid overlay slug", str(ctx.exception))

    def test_load_overlay_accepts_normal_slug_after_hardening(self) -> None:
        # Hyphenated shipped types must NOT be rejected (the guard never
        # checks "-"), in both the shipped and consumer tiers.
        for shipped in MEMO_ARTIFACT_TYPES:
            with self.subTest(tier="shipped", artifact_type=shipped):
                overlay = load_overlay(shipped)
                self.assertEqual(
                    str(overlay.artifact_type), shipped.value
                )
        consumer_slug = "design-note"
        (self.consumer_dir / f"{consumer_slug}.json").write_text(
            json.dumps(
                {
                    "artifact_type": consumer_slug,
                    "description": "Consumer-tier sanity slug.",
                    "weight_adjustments": {},
                    "calibration_prose": {},
                }
            ),
            encoding="utf-8",
        )
        overlay = load_overlay(
            consumer_slug, consumer_overlays_dir=self.consumer_dir
        )
        self.assertEqual(overlay.artifact_type, consumer_slug)


class TestDocCanonicalReferences(unittest.TestCase):
    """The rubric overlay surface is documented in SKILL.md, rubric.md, and memo-review.md."""

    SKILL_ROOT = Path(__file__).resolve().parent.parent

    def test_skill_md_documents_overlay_system(self) -> None:
        body = (self.SKILL_ROOT / "SKILL.md").read_text(encoding="utf-8")
        self.assertIn("rubric_overlays", body)

    def test_rubric_md_documents_overlay_system(self) -> None:
        body = (self.SKILL_ROOT / "rubric.md").read_text(encoding="utf-8")
        self.assertIn("rubric_overlays", body)

    def test_memo_review_command_invokes_overlay_selection(self) -> None:
        body = (self.SKILL_ROOT / "commands" / "memo-review.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("select_overlay_for_thread", body)

    def test_memo_review_command_documents_skill_mismatch_loud_failure(self) -> None:
        """Step 4i documents the post-#386 loud-failure outcome (issue #390).

        Skill-identity artifact types (anything outside MEMO_ARTIFACT_TYPES)
        raise OverlayLoadError rather than returning None; the command prose
        must name both symbols so the contract can't silently regress.
        """
        body = (self.SKILL_ROOT / "commands" / "memo-review.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("OverlayLoadError", body)
        self.assertIn("MEMO_ARTIFACT_TYPES", body)


if __name__ == "__main__":
    unittest.main()
