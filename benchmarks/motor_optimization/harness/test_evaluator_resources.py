"""Real subprocess lifecycle checks; no FEM performance evidence."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import evaluator  # noqa: E402


FAKE = r'''
import json,os,subprocess,sys,time
from pathlib import Path
mode, record = sys.argv[1:]
Path(record).write_text(str(os.getpid()))
if mode == 'bad':
    print('{invalid-json',flush=True)
    time.sleep(30)
elif mode == 'eof':
    sys.exit(3)
elif mode == 'timeout':
    time.sleep(30)
else:
    print(json.dumps(dict(op='ready',ok=True,resources={'marker':'startup'})),flush=True)
    for line in sys.stdin:
        r=json.loads(line)
        if r['op']=='hang': time.sleep(30)
        if r['op']=='work':
            subprocess.run([sys.executable,'-c',
                'import time; e=time.process_time()+.04\nwhile time.process_time()<e: pass'],check=True)
        print(json.dumps(dict(op=r['op'],id=r['id'],ok=True,
                             resources={'marker':r['op']})),flush=True)
        if r['op']=='shutdown':break
'''


class LifecycleTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.script = self.root / "fake.py"
        self.script.write_text(FAKE)
        self.pidfile = self.root / "pid"
        self.children = []

    def tearDown(self):
        # Guard test failures without leaving a fixture alive.
        for p in self.children:
            if p.returncode is None:
                try:
                    os.killpg(p.pid, 9)
                except ProcessLookupError:
                    pass
                p.wait(timeout=5)
        self.tmp.cleanup()

    def create(self, mode="ok", timeout=2, journal=None):
        popen = subprocess.Popen

        def capture(*args, **kwargs):
            proc = popen(*args, **kwargs)
            self.children.append(proc)
            return proc

        with patch.object(evaluator.subprocess, "Popen", side_effect=capture):
            return evaluator.Evaluator(
                [sys.executable, "-I", str(self.script), mode, str(self.pidfile)],
                stderr_path=self.root / "stderr.log", timeout=timeout, process_journal=journal)

    def assert_closed(self, proc):
        self.assertIsNotNone(proc.returncode)
        self.assertTrue(proc.stdin.closed)
        self.assertTrue(proc.stdout.closed)
        with self.assertRaises(ChildProcessError):
            os.waitpid(proc.pid, os.WNOHANG)

    def test_failed_process_journal_cannot_skip_constructor_cleanup(self):
        class Broken:
            def emit(self, kind, **fields):
                if kind == "process_spawn_intent": return 0
                raise OSError("fixture lifecycle journal failure")
        with self.assertRaisesRegex(OSError, "lifecycle journal failure") as error:
            self.create(journal=Broken())
        self.assert_closed(self.children[-1])
        receipt = error.exception.evaluator_resources["child_receipt"]
        self.assertEqual(receipt["state"], "reaped")
        self.assertIn("tracking_error", receipt)

    def test_constructor_failures_reap_owned_child_and_preserve_receipt(self):
        for mode, expected in (("bad", json.JSONDecodeError),
                               ("eof", evaluator.EvaluatorFailure),
                               ("timeout", evaluator.Aborted)):
            with self.subTest(mode=mode):
                with self.assertRaises(expected) as error:
                    self.create(mode, timeout=.3 if mode == "timeout" else 2)
                self.assert_closed(self.children[-1])
                receipt = error.exception.evaluator_resources["child_receipt"]
                self.assertEqual(receipt["state"], "reaped")
                self.assertFalse(receipt["cpu_complete"])
                json.dumps(error.exception.evaluator_resources, allow_nan=False)

    def test_launch_failure_closes_log_without_inventing_child_usage(self):
        opened = []
        real_open = open

        def record(*args, **kwargs):
            f = real_open(*args, **kwargs)
            opened.append(f)
            return f

        with patch("builtins.open", side_effect=record):
            with self.assertRaises(FileNotFoundError) as error:
                evaluator.Evaluator([str(self.root / "missing")],
                                    stderr_path=self.root / "launch.log", timeout=1)
        self.assertTrue(opened)
        self.assertTrue(all(f.closed for f in opened))
        self.assertIsNone(error.exception.evaluator_resources["child_receipt"])

    def test_normal_exit_preserves_one_inclusive_child_receipt_and_requests(self):
        ev = self.create()
        try:
            ev.request("work", timeout=5)
            self.assertIsNone(ev.proc.returncode)
        finally:
            ev.close()
        receipt = ev.receipt
        self.assert_closed(ev.proc)
        self.assertTrue(ev._err.closed)
        self.assertEqual(receipt["returncode"], 0)
        if hasattr(os, "wait4"):
            self.assertTrue(receipt["cpu_complete"])
            self.assertGreaterEqual(receipt["cpu"]["total_seconds"], .04)
        self.assertEqual([r["op"] for r in ev.resources()["requests"]],
                         ["startup", "work", "shutdown"])
        self.assertEqual([r["measurement"]["marker"] for r in ev.resources()["requests"]],
                         ["startup", "work", "shutdown"])
        ev.close()
        self.assertIs(ev.receipt, receipt)

    def test_interrupted_request_is_unavailable_and_forced_receipt_incomplete(self):
        ev = self.create()
        try:
            with self.assertRaises(evaluator.Aborted):
                ev.request("hang", timeout=.05)
        finally:
            ev.close()
        self.assert_closed(ev.proc)
        self.assertFalse(ev.receipt["cpu_complete"])
        last = ev.request_resources[-1]
        self.assertEqual(last["op"], "hang")
        self.assertFalse(last["measurement"]["available"])

    def test_run_trace_retains_startup_failure_receipt_before_object_assignment(self):
        import run
        import test_harness as fixtures

        anchor = self.root / "anchor.json"
        anchor.write_text(json.dumps({"anchor": fixtures.anchor_json()}))
        session = {"label": "test", "budget_fe": 1.0, "unit_seconds": 1.0,
                   "weights": fixtures.WEIGHTS, "theta0_train": {},
                   "evaluator": {"anchor": str(anchor)}}
        for mode in ("bad", "timeout"):
            with self.subTest(mode=mode):
                target = self.root / f"{mode}.json"
                trace = run.run_one(
                    session, "slsqp", "a", "theta0", 1, target,
                    evaluator_factory=lambda space, ledger: self.create(mode, timeout=.1),
                    diagnostic=False)
                saved = json.loads(target.read_text())
                self.assertEqual(saved["resources"]["evaluator"]["child_receipt"]["state"],
                                 "reaped")
                self.assertFalse(saved["resources"]["cpu_complete"])
                self.assertEqual(trace["stop_reason"], "budget" if mode == "timeout" else "error")
                self.assert_closed(self.children[-1])

    def test_diagnostic_startup_failure_retains_separate_resource_evidence(self):
        import run

        with patch.object(run, "c_current_predictions", return_value={"x": [0.0]}):
            result = run.post_run_diagnostic(object(), lambda: self.create("bad"), 1.0)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["counted_in_run_budget"])
        self.assertEqual(result["resources"]["label"], "post_run_diagnostic")
        self.assertEqual(result["resources"]["evaluator"]["child_receipt"]["state"], "reaped")
        self.assert_closed(self.children[-1])

    def test_calibration_retains_distinct_evaluators_and_one_batch_total(self):
        import itertools
        import session

        created = []
        numbers = itertools.count()

        class Calibration:
            def __init__(self, *args, **kwargs):
                self.number = next(numbers)
                self.ready = {"space": {"x0": [0.0]}}
                self.closed = False
                created.append(self)

            def request(self, op, **kwargs):
                if op == "train":
                    return {"F": 1.0, "mean_full": 2.0}
                return {"ok": True, "validation_seconds": [2.0] * 3,
                        "train_forward_seconds": [1.0] * 3,
                        "train_adjoint_seconds": [.5] * 3, "unit_seconds": 2.0,
                        "weights": {"validation_solves_per_eval": 60, "train_solves_per_eval": 60},
                        "theta0_train": {"F": 1.0, "mean_full": 2.0},
                        "theta0_validation": {}, "anchor_agrees_bitwise": True,
                        "anchor_agrees_4ulp": True, "deterministic": True}

            def close(self):
                self.closed = True

            def resources(self):
                assert self.closed, "the receipt must be captured after cleanup"
                return {"fixture_id": self.number, "child_receipt": {"cpu_complete": True}}

        with patch.object(session, "Evaluator", Calibration):
            result = session.calibrate("fixture", "train", "val", 0, 0, "anchor", self.root, 2)
        self.assertEqual(len(created), 4)  # single, two concurrent, space B
        receipts = [result["single_process"]["process_resources"], result["space_b_setup_resources"]]
        receipts += [p["process_resources"] for p in result["per_process"]]
        self.assertEqual({p["fixture_id"] for p in receipts}, set(range(4)))
        self.assertTrue(all(p.closed for p in created))
        self.assertEqual(result["resources"]["label"], "whole_calibration_batch")
        self.assertEqual(result["unit_seconds"], 2.0)


if __name__ == "__main__":
    unittest.main()
