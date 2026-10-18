"""Diagnostic repairs preserve stopped outcomes, bindings and child cleanup."""
import copy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import diagnostic_repair as repair
import run as runner
from interruption import Interrupted, StopSignals
from session_journal import digest, read_events
from test_harness import FakeClock, FakeEvaluator
from test_run_interruption import fixture


def stopped_fixture(root, method="slsqp_l0"):
    clock = FakeClock()
    session = fixture(root)
    trace = runner.run_one(session, method, "a", "theta0", 1, root / "trace.json",
        clock=clock, evaluator_factory=lambda sp, led: FakeEvaluator(clock))
    return session, root / "trace.optimizer-stopped.json", trace


class RepairTests(unittest.TestCase):
    def test_repair_refuses_substituted_training_mesh_and_keeps_stopped_trace(self):
        for method in ("surrogate", "cobyqa_l0", "slsqp_l0"):
            with self.subTest(method=method), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                session, snapshot, _ = stopped_fixture(root, method)
                before = snapshot.read_bytes()
                clock = FakeClock(); ev = FakeEvaluator(clock, train_sha="1" * 64)
                result = repair.execute(snapshot, digest(snapshot), session, root / "repair",
                    attempt_id="wrong-training-mesh", clock=clock, evaluator_factory=lambda journal: ev)
                self.assertEqual(result["status"], "failed")
                self.assertIn("training mesh", result["diagnostic"]["error"])
                self.assertEqual(result["diagnostic"]["evaluator_ready"], ev.ready)
                self.assertEqual(ev.requests, [])
                self.assertTrue(ev.closed)
                self.assertEqual(snapshot.read_bytes(), before)

    def test_repair_matches_original_diagnostic_without_optimizer_or_ledger(self):
        for method in ("surrogate", "cobyqa_l0", "slsqp_l0"):
            with self.subTest(method=method), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                session, snapshot, trace = stopped_fixture(root, method)
                before = {p: p.read_bytes() for p in root.glob("*.json")}
                clock = FakeClock()
                with mock.patch.object(runner, "execute", side_effect=AssertionError("optimizer rerun")), \
                     mock.patch.object(runner, "Ledger", side_effect=AssertionError("new ledger")):
                    record = repair.execute(snapshot, digest(snapshot), session, root / "repair",
                        attempt_id="recovery1:diagnostic:0", clock=clock,
                        evaluator_factory=lambda journal: FakeEvaluator(clock))
                self.assertEqual(record["status"], "ok")
                self.assertFalse(record["optimizer_reexecuted"])
                self.assertFalse(record["counted_in_run_budget"])
                self.assertEqual(record["unit_seconds"], session["unit_seconds"])
                for key in ("rows", "prediction" if method == "surrogate" else "selection"):
                    self.assertEqual(record["diagnostic"][key], trace["post_run_diagnostic"][key])
                self.assertGreater(record["wall_seconds"], 0)
                for path, raw in before.items():
                    self.assertEqual(path.read_bytes(), raw, path)
                self.assertEqual(json.loads((root / "repair/terminal.json").read_text()), record)
                events = read_events(root / "repair/events.jsonl")["events"]
                self.assertEqual(events[-1]["kind"], "diagnostic_finished")

    def test_changed_snapshot_session_unit_model_anchor_or_completion_refuses_before_launch(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            session, snapshot, _ = stopped_fixture(root)
            original = snapshot.read_bytes()
            expected = digest(snapshot)
            cases = [("hash", None, None)]
            for key, value in (("session_id", "other"), ("unit_seconds", 2.), ("unit_seconds", True),
                               ("budget_fe", 123.), ("label", "other")):
                changed = copy.deepcopy(session); changed[key] = value
                cases.append((key + str(value), changed, None))
            changed = copy.deepcopy(session); changed["models"]["L0"]["train_level"] = 1
            cases.append(("model", changed, None))
            for key, value in (("optimizer_completed", False), ("stop_reason", "interrupted"),
                               ("diagnostic_status", "ok")):
                changed = json.loads(original); changed[key] = value
                cases.append((key, None, changed))
            changed = json.loads(original); changed["anchor"]["msh_sha256"] = "changed"
            cases.append(("anchor", None, changed))
            for index, (name, changed_session, changed_snapshot) in enumerate(cases):
                snapshot.write_bytes(original)
                if changed_snapshot:
                    snapshot.write_text(json.dumps(changed_snapshot))
                binding = "0" * 64 if name == "hash" else digest(snapshot)
                factory = mock.Mock(side_effect=AssertionError("launched invalid repair"))
                with self.subTest(name=name), self.assertRaises(ValueError):
                    repair.execute(snapshot, binding, changed_session or session, root / f"repair{index}",
                                   attempt_id="attempt", evaluator_factory=factory)
                factory.assert_not_called()
                self.assertFalse((root / f"repair{index}").exists())
            snapshot.write_bytes(original)
            self.assertEqual(digest(snapshot), expected)

    def test_output_reuse_cannot_replace_an_earlier_diagnostic_failure(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            session, snapshot, _ = stopped_fixture(root)
            def failed(journal):
                error = RuntimeError("startup failure")
                error.evaluator_resources = {"child_receipt": {"cpu_complete": False}}
                raise error
            result = repair.execute(snapshot, digest(snapshot), session, root / "repair",
                                    attempt_id="first", evaluator_factory=failed)
            self.assertEqual(result["status"], "failed")
            self.assertIn("startup failure", result["diagnostic"]["error"])
            self.assertFalse(result["diagnostic"]["resources"]["evaluator"]["child_receipt"]["cpu_complete"])
            before = {p: p.read_bytes() for p in (root / "repair").iterdir() if p.is_file()}
            with self.assertRaises(FileExistsError):
                repair.execute(snapshot, digest(snapshot), session, root / "repair", attempt_id="second")
            for path, raw in before.items():
                self.assertEqual(path.read_bytes(), raw)

    def test_preparation_interrupt_and_failure_are_separate_recorded_costs(self):
        for failure in (Interrupted(signal.SIGTERM), RuntimeError("preparation failed")):
            with self.subTest(failure=type(failure).__name__), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                session, snapshot, _ = stopped_fixture(root, "surrogate")
                before = snapshot.read_bytes()
                clock = FakeClock()
                def fail(run):
                    clock.advance(.5)
                    raise failure
                with mock.patch.object(runner, "c_current_predictions", fail):
                    record = repair.execute(snapshot, digest(snapshot), session, root / "repair",
                                            attempt_id="repair", clock=clock)
                self.assertEqual(record["wall_seconds"], .5)
                self.assertEqual(record["status"], "interrupted" if isinstance(failure, Interrupted) else "failed")
                self.assertFalse(record["counted_in_run_budget"])
                self.assertEqual(snapshot.read_bytes(), before)
                self.assertIn("resources", record["diagnostic"])

    def test_pending_stop_prevents_evaluator_launch(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            session, snapshot, _ = stopped_fixture(root)
            factory = mock.Mock(side_effect=AssertionError("launch despite pending stop"))
            with StopSignals() as signals:
                signals.requested = Interrupted(signal.SIGTERM)
                record = repair.execute(snapshot, digest(snapshot), session, root / "repair",
                    attempt_id="pending", signals=signals, evaluator_factory=factory)
            factory.assert_not_called()
            self.assertEqual(record["status"], "interrupted")
            self.assertFalse(record["process_interruption"]["verified_external_interruption"])

    def test_real_signal_reaps_tracked_evaluator_and_preserves_original(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            session, snapshot, _ = stopped_fixture(root)
            before = snapshot.read_bytes()
            (root / "frozen-session.json").write_text(json.dumps(session))
            (root / "server.py").write_text('''import json, os, pathlib, subprocess, sys, time
root=pathlib.Path(sys.argv[1])
child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"])
(root/"owned.json").write_text(json.dumps([os.getpid(),child.pid]))
print(json.dumps({"op":"ready","ok":True}),flush=True)
for line in sys.stdin:
    (root/"waiting").touch()
    time.sleep(60)
''')
            (root / "worker.py").write_text('''import json,pathlib,sys
sys.path.insert(0,sys.argv[1])
import diagnostic_repair as repair
from evaluator import Evaluator
from interruption import StopSignals
from session_journal import digest
from test_harness import FakeClock, FakeEvaluator
root=pathlib.Path(sys.argv[2]); snapshot=root/"trace.optimizer-stopped.json"
session=json.loads((root/"frozen-session.json").read_text())
class Bridge(FakeEvaluator):
    def __init__(self,journal):
        super().__init__(FakeClock())
        self.real=Evaluator([sys.executable,str(root/"server.py"),str(root)],
                            timeout=30,process_journal=journal)
    def request(self,op,**kw): return self.real.request("wait",timeout=30)
    def kill(self): self.real.kill()
    def close(self): self.real.close()
    def resources(self): return self.real.resources()
with StopSignals() as signals:
    result=repair.execute(snapshot,digest(snapshot),session,root/"repair",
        attempt_id="real-signal",signals=signals,evaluator_factory=Bridge)
sys.exit(128+result["process_interruption"]["signal"])
''')
            proc = subprocess.Popen([sys.executable, "-I", str(root / "worker.py"), str(HERE), str(root)],
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            owned = []
            try:
                deadline = time.monotonic() + 15
                while not (root / "waiting").exists() and time.monotonic() < deadline:
                    if proc.poll() is not None:
                        self.fail(proc.communicate()[1].decode())
                    time.sleep(.01)
                self.assertTrue((root / "waiting").exists())
                owned = json.loads((root / "owned.json").read_text())
                os.kill(proc.pid, signal.SIGTERM)
                _, stderr = proc.communicate(timeout=10)
                self.assertEqual(proc.returncode, 128 + signal.SIGTERM, stderr.decode())
                result = json.loads((root / "repair/terminal.json").read_text())
                self.assertEqual(result["status"], "interrupted")
                receipt = result["diagnostic"]["resources"]["evaluator"]["child_receipt"]
                self.assertEqual(receipt["state"], "reaped")
                self.assertFalse(receipt["cpu_complete"])
                self.assertEqual(snapshot.read_bytes(), before)
                import process_verification
                verification = process_verification.verify(root / "repair")
                self.assertTrue(verification["processes_stopped"], verification["issues"])
                events = read_events(root / "repair/events.jsonl")["events"]
                for kind in ("process_spawn_intent", "process_spawn_started", "process_spawn_reaped"):
                    self.assertEqual(sum(e["kind"] == kind for e in events), 1)
            finally:
                if proc.poll() is None:
                    proc.kill(); proc.communicate()
                if owned:
                    try: os.killpg(owned[0], signal.SIGKILL)
                    except ProcessLookupError: pass


if __name__ == "__main__":
    unittest.main(verbosity=2)
