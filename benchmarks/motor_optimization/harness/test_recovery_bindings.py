"""Recovery refuses changed runtime/source/input state without recalibration."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import recovery_bindings as bindings
import runtime_fingerprint


def fixture(root):
    for name in ("binary", "L0", "L1", "L2", "anchor", "starts"):
        (root / name).write_text(name)
    return {"label": "main", "session_id": "original", "procs": 2,
            "unit_seconds": 3., "budget_fe": 120., "starts": str(root / "starts"),
            "models": {f"L{k}": {"train_mesh": str(root / f"L{k}"), "train_level": k}
                       for k in (0, 1)},
            "evaluator": {"binary": str(root / "binary"), "val_mesh": str(root / "L2"),
                          "anchor": str(root / "anchor")}}


class BindingTests(unittest.TestCase):
    def test_same_record_reuses_unit_without_launch_or_calibration(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); session = fixture(root)
            kwargs = {"repo": root, "source_probe": lambda repo: {"commit": "abc", "tree_clean": True},
                      "runtime_probe": lambda repo: {"packages": "frozen"}}
            frozen = bindings.capture(session, **kwargs)
            # The checker has no calibration/optimizer dependency to call.
            with mock.patch("session.calibrate", side_effect=AssertionError("recalibrated")), \
                 mock.patch("run.run_one", side_effect=AssertionError("launched")):
                result = bindings.verify(frozen, session, **kwargs)
            self.assertTrue(result["verified"])
            self.assertFalse(result["can_launch"])
            self.assertEqual(frozen["unit_seconds"], 3.)

    def test_each_external_input_change_is_detected(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); session = fixture(root)
            kwargs = {"repo": root, "source_probe": lambda repo: {"tree_clean": True},
                      "runtime_probe": lambda repo: {}}
            frozen = bindings.capture(session, **kwargs)
            for name in ("binary", "L0", "L1", "L2", "anchor", "starts"):
                path = root / name; original = path.read_bytes()
                path.write_bytes(original + b" changed")
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, "inputs"):
                    bindings.verify(frozen, session, **kwargs)
                path.write_bytes(original)
            (root / "L0").unlink()
            with self.assertRaises(FileNotFoundError):
                bindings.verify(frozen, session, **kwargs)

    def test_changed_unit_workers_settings_source_or_runtime_refuses(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); session = fixture(root)
            source = {"commit": "abc", "tree_clean": True, "files": {"x.py": "first"}}
            runtime = {"harness": "one", "remesher": "two", "uv": "three"}
            kwargs = {"repo": root, "source_probe": lambda repo: copy.deepcopy(source),
                      "runtime_probe": lambda repo: copy.deepcopy(runtime)}
            frozen = copy.deepcopy(bindings.capture(session, **kwargs))
            for key, value in (("unit_seconds", 4.), ("unit_seconds", True), ("procs", 3),
                               ("procs", True), ("budget_fe", 100.), ("session_id", "new")):
                changed = copy.deepcopy(session); changed[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    bindings.verify(frozen, changed, **kwargs)
            for key in runtime:
                original = runtime[key]; runtime[key] = "changed"
                with self.subTest(runtime=key), self.assertRaisesRegex(ValueError, "runtime"):
                    bindings.verify(frozen, session, **kwargs)
                runtime[key] = original
            for key, value in (("commit", "def"), ("files", {"x.py": "changed"}), ("tree_clean", False)):
                original = source[key]; source[key] = value
                with self.subTest(source=key), self.assertRaises(ValueError):
                    bindings.verify(frozen, session, **kwargs)
                source[key] = original
            frozen["source"]["tree_clean"] = False
            with self.assertRaisesRegex(ValueError, "clean"):
                bindings.verify(frozen, session, **kwargs)
            frozen["source"]["tree_clean"] = True
            frozen["host"]["host"] = "a different machine"
            with self.assertRaisesRegex(ValueError, "host"):
                bindings.verify(frozen, session, **kwargs)

    def test_real_git_manifest_observes_dirty_content_new_sources_and_failures(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)
            git("init", "-q")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            (root / "crates").mkdir(); path = root / "crates/model.rs"; path.write_text("first")
            git("add", "."); git("commit", "-qm", "fixture")
            original = bindings.sources(root)
            self.assertTrue(original["tree_clean"])
            output = root / "benchmarks/motor_optimization/runs/main"
            output.mkdir(parents=True)
            (output / "owner.lock").touch()
            (output / "events.jsonl").write_text("setup evidence")
            (output / "session.json").write_text("{}")
            self.assertEqual(bindings.sources(root), original)
            path.write_text("changed")
            changed = bindings.sources(root)
            self.assertFalse(changed["tree_clean"])
            self.assertNotEqual(original["files"], changed["files"])
            path.write_text("first")
            (root / "crates/new.rs").write_text("new")
            self.assertFalse(bindings.sources(root)["tree_clean"])
            with self.assertRaises(RuntimeError):
                bindings.sources(root.parent)

    def test_installed_payload_is_hashed_not_only_package_version(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); path = root / "module.py"; path.write_text("first")
            class Dist:
                version = "1.0"
                files = [Path("module.py")]
                def locate_file(self, entry): return root / entry
            with mock.patch.object(runtime_fingerprint.importlib.metadata, "distribution", return_value=Dist()):
                first = runtime_fingerprint.capture(["fixture"])
                path.write_text("changed")
                second = runtime_fingerprint.capture(["fixture"])
                self.assertEqual(first["distributions"]["fixture"]["version"], "1.0")
                self.assertNotEqual(first, second)
                path.unlink()
                with self.assertRaises(ValueError): runtime_fingerprint.capture(["fixture"])

    def test_missing_uv_or_failed_runtime_probe_never_returns_verification(self):
        with mock.patch.object(bindings.shutil, "which", return_value=None):
            with self.assertRaisesRegex(ValueError, "uv runtime"):
                bindings.runtime()
        with mock.patch.object(bindings, "command", side_effect=RuntimeError("probe failed")):
            with self.assertRaisesRegex(RuntimeError, "probe failed"):
                bindings.runtime()

    def test_sources_and_primary_inputs_cannot_change_during_calibration(self):
        source = {"commit": "first", "files": {"a": "b"}, "tree_clean": True}
        provenance = {"source": source, "evaluator": {"sha256": "binary"}, "runtime": {"python": "one"}}
        meshes = {key: {"sha256": key} for key in ("train", "validation", "anchor", "starts")}
        inputs = {key: {"sha256": sha} for key, sha in (
            ("binary", "binary"), ("training_L1", "train"), ("validation_mesh", "validation"),
            ("anchor", "anchor"), ("starts", "starts"))}
        bound = {"source": copy.deepcopy(source), "session": {"primary_model": "L1"}, "inputs": inputs,
                 "runtime": {"python": "one"}}
        bindings.validate_setup(bound, provenance, meshes)
        for name in inputs:
            bad = copy.deepcopy(bound); bad["inputs"][name]["sha256"] = "changed"
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "changed during setup"):
                bindings.validate_setup(bad, provenance, meshes)
        bad = copy.deepcopy(bound); bad["source"]["commit"] = "changed"
        with self.assertRaisesRegex(ValueError, "sources changed"):
            bindings.validate_setup(bad, provenance, meshes)
        bad = copy.deepcopy(bound); bad["runtime"]["python"] = "changed"
        with self.assertRaisesRegex(ValueError, "runtime changed"):
            bindings.validate_setup(bad, provenance, meshes)


if __name__ == "__main__":
    unittest.main(verbosity=2)
