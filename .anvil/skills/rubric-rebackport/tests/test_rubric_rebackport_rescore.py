"""Tests for `anvil:rubric-rebackport` rescore-mode primitives (issue #358).

Coverage:

- Rescore mode plans the correct sidecar path
  (``.rescore-<target-id>/``).
- The legacy review dir is byte-identical after a rescore apply.
- ``allow_rescore_subprocess=False`` defers every rescore.
- When the per-skill reviewer command lacks the ``--rescore-mode``
  hook (the default state for now), rescue is recorded as deferred.
- When the hook check is satisfied, the placeholder sidecar is
  written with the expected ``_meta.json`` shape.
- ``--rescore`` without ``--legacy-rubric`` raises at the orchestrate
  layer.
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

from _skill_lib import apply_mod, detect, orchestrate, plan, rescore  # noqa: E402
from _rebackport_fixtures import build_legacy_unstamped  # noqa: E402

apply_plan = apply_mod.apply_plan
inventory_tree = detect.inventory_tree
run = orchestrate.run
Mode = plan.Mode
build_plan = plan.build_plan
check_rescore_hook = rescore.check_rescore_hook
invoke_rescore = rescore.invoke_rescore


class TestRescoreSidecarPath(unittest.TestCase):
    def test_sidecar_path_naming_convention(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
            )
            rp = p.reviews[0]
            self.assertIsNotNone(rp.rescore_spec)
            self.assertEqual(
                rp.rescore_spec.sidecar_path.parent,
                rp.review_dir.parent,
            )
            expected = (
                f"{rp.review_dir.name}.rescore-anvil-memo-v2"
            )
            self.assertEqual(
                rp.rescore_spec.sidecar_path.name, expected
            )


class TestRescoreLegacyDirByteIdentity(unittest.TestCase):
    def test_legacy_review_dir_untouched_after_rescore(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            review_dir = inv.reviews[0].review_dir
            before = {}
            for f in sorted(review_dir.rglob("*")):
                if f.is_file():
                    rel = str(f.relative_to(review_dir))
                    before[rel] = hashlib.sha256(f.read_bytes()).hexdigest()
            run(
                project,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
                apply=True,
                allow_rescore_subprocess=True,
            )
            after = {}
            for f in sorted(review_dir.rglob("*")):
                if f.is_file():
                    rel = str(f.relative_to(review_dir))
                    after[rel] = hashlib.sha256(f.read_bytes()).hexdigest()
            self.assertEqual(
                before, after,
                "rescore mutated the legacy review dir (must be untouched)",
            )


class TestRescoreDeferralBehavior(unittest.TestCase):
    def test_allow_subprocess_false_defers_all(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
            )
            result = apply_plan(p, allow_rescore_subprocess=False)
            self.assertEqual(len(result.deferred_reviews), 1)
            self.assertEqual(len(result.applied_reviews), 0)
            spec = p.reviews[0].rescore_spec
            self.assertFalse(spec.sidecar_path.exists())

    def test_check_rescore_hook_returns_true_for_migrated_skills(
        self,
    ) -> None:
        # All eight skill review commands carry the `--rescore-mode`
        # token per issue #368 (the per-skill reviewer-hook landing
        # PR). check_rescore_hook(skill) must report the hook as
        # present for each.
        for skill in (
            "memo",
            "proposal",
            "paper",  # renamed from `pub` under #694
            "deck",
            "slides",
            "report",
            "ip-uspto",
            "installation",
        ):
            with self.subTest(skill=skill):
                self.assertTrue(
                    check_rescore_hook(skill),
                    f"--rescore-mode hook expected to be present in "
                    f"{skill}-review.md (issue #368)",
                )

    def test_check_rescore_hook_returns_true_when_hook_present(
        self,
    ) -> None:
        with TemporaryDirectory() as td:
            skill_root = Path(td)
            (skill_root / "myskill" / "commands").mkdir(parents=True)
            (skill_root / "myskill" / "commands" / "myskill-review.md").write_text(
                "# myskill-review\n\nSupports --rescore-mode for "
                "rebackport.\n"
            )
            self.assertTrue(
                check_rescore_hook("myskill", skill_root=skill_root)
            )
            self.assertFalse(
                check_rescore_hook("absent-skill", skill_root=skill_root)
            )


class TestRescorePlaceholderSidecar(unittest.TestCase):
    def test_invoke_rescore_writes_placeholder_when_hook_present(
        self,
    ) -> None:
        with TemporaryDirectory() as td:
            skill_root = Path(td) / "fake-skills"
            (skill_root / "memo" / "commands").mkdir(parents=True)
            (skill_root / "memo" / "commands" / "memo-review.md").write_text(
                "Supports --rescore-mode for rebackport.\n"
            )
            project = build_legacy_unstamped(Path(td))
            inv = inventory_tree(project)
            p = build_plan(
                inv,
                mode=Mode.RESCORE,
                legacy_rubric="anvil-memo-v1-legacy-40",
            )
            spec = p.reviews[0].rescore_spec
            outcome = invoke_rescore(spec, skill_root=skill_root)
            self.assertTrue(outcome.written)
            self.assertTrue(spec.sidecar_path.is_dir())
            meta_path = spec.sidecar_path / "_meta.json"
            self.assertTrue(meta_path.is_file())
            data = json.loads(meta_path.read_text())
            self.assertEqual(data["rubric_id"], "anvil-memo-v2")
            self.assertEqual(data["rubric_total"], 44)
            self.assertEqual(data["advance_threshold"], 35)
            self.assertEqual(
                data["prior_rubric_id"], "anvil-memo-v1-legacy-40"
            )
            self.assertEqual(
                data["rescore_source"], "anvil:rubric-rebackport"
            )
            self.assertEqual(data["rescore_state"], "scheduled")


class TestRescoreOrchestrationValidation(unittest.TestCase):
    def test_rescore_without_legacy_rubric_raises(self) -> None:
        with TemporaryDirectory() as td:
            project = build_legacy_unstamped(Path(td))
            with self.assertRaises(ValueError):
                run(project, mode=Mode.RESCORE, apply=True)


if __name__ == "__main__":
    unittest.main()
