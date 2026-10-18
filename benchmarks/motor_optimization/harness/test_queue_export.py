"""Prelaunch queue inspection uses the real execution registry; no FEM."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import driver
import export_queue
import main_schedule
import session


class QueueExportTests(unittest.TestCase):
    def test_real_cli_complete_queue_hashes_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "plan.json"
            argv = [sys.executable, "-I", str(HERE / "export_queue.py"), "--out", str(out)]
            run = subprocess.run(argv, capture_output=True, text=True, timeout=120)
            self.assertEqual(run.returncode, 0, run.stderr)
            raw = out.read_bytes()
            plan = json.loads(raw)
            receipt = json.loads(run.stdout)
            self.assertEqual(receipt["sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual(plan["method_specs"], session.METHOD_SPECS)
            self.assertEqual(plan["jobs"], main_schedule.registered_jobs(session.METHOD_SPECS))
            self.assertEqual(len(plan["jobs"]), 100)
            self.assertEqual(len({j["name"] for j in plan["jobs"]}), 100)
            for key in ("jobs", "method_specs"):
                encoded = json.dumps(plan[key], sort_keys=True, separators=(",", ":")).encode()
                self.assertEqual(plan[key + "_sha256"], hashlib.sha256(encoded).hexdigest())
            for name in ("driver.py", "session.py", "main_schedule.py", "export_queue.py", "starts.json"):
                self.assertEqual(plan["source"]["files"]["benchmarks/motor_optimization/harness/" + name],
                                 hashlib.sha256((HERE / name).read_bytes()).hexdigest())
            self.assertEqual(len(plan["source"]["commit"]), 40)
            again = subprocess.run(argv, capture_output=True, text=True, timeout=120)
            self.assertNotEqual(again.returncode, 0)
            self.assertIn("FileExistsError", again.stderr)
            self.assertEqual(out.read_bytes(), raw)
            self.assertEqual(list(Path(td).iterdir()), [out])

    def test_changed_sources_refused_before_creating_output(self):
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "plan.json"
            with patch.object(export_queue.recovery_bindings, "sources", side_effect=[
                    {"commit": "a" * 40}, {"commit": "b" * 40}]), self.assertRaisesRegex(
                    ValueError, "sources changed"):
                export_queue.export(out)
            self.assertFalse(out.exists())

    def test_export_reports_dirty_sources_without_claiming_launch_readiness(self):
        source = {"commit": "a" * 40, "tree_clean": False, "files": {}, "untracked_sources": []}
        with tempfile.TemporaryDirectory() as td, patch.object(
                export_queue.recovery_bindings, "sources", return_value=source), patch.object(
                session, "calibrate", side_effect=AssertionError("must not calibrate")), patch.object(
                session, "Evaluator", side_effect=AssertionError("must not evaluate")):
            out = Path(td) / "plan.json"
            receipt = export_queue.export(out)
            self.assertFalse(receipt["source_tree_clean"])
            self.assertEqual(json.loads(out.read_text())["label"], "main_queue_plan")

    def test_missing_duplicate_and_extra_method_specs_are_refused(self):
        missing = copy.deepcopy(driver.METHOD_SPECS)
        missing.pop("slsqp_l0")
        duplicate = copy.deepcopy(driver.METHOD_SPECS)
        duplicate["slsqp_l0"]["letter"] = "A"
        extra = copy.deepcopy(driver.METHOD_SPECS)
        extra["alias"] = copy.deepcopy(extra["slsqp"])
        for specs in (missing, duplicate, extra):
            with self.subTest(specs=specs), self.assertRaises(ValueError):
                main_schedule.registered_jobs(specs)


if __name__ == "__main__":
    unittest.main()
