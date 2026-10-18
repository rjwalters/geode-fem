"""Stopped-run persistence, real signal cleanup and diagnostic input recovery."""
import copy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from unittest import mock
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import run as runner
from interruption import Interrupted, StopSignals
from session_journal import digest
from test_harness import FakeClock, FakeEvaluator, WEIGHTS, X0, anchor_json, theta0_train


def fixture(root):
    (root / "anchor.json").write_text(json.dumps({"anchor": anchor_json()}))
    (root / "starts.json").write_text(json.dumps({"starts": {"theta0": {"a": X0}}}))
    train = theta0_train(FakeEvaluator(FakeClock()))
    return {"label": "test", "session_id": "interruption-fixture", "unit_seconds": 1.,
            "budget_fe": 400., "primary_model": "L1",
            "models": {f"L{k}": {"train_level": k, "train_mesh": "fixture.msh",
                       "weights": WEIGHTS, "theta0_train": train} for k in (0, 1)},
            "starts": str(root / "starts.json"),
            "evaluator": {"anchor": str(root / "anchor.json")}}


def successful_prefix(run, u):
    point = u + .01
    run.train(point, False)
    run.validate(point)
    return "converged", "fixture", None


class InterruptionTests(unittest.TestCase):
    def test_stopped_snapshot_exists_before_diagnostic_and_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as td:
            root, clock = Path(td), FakeClock()
            session = fixture(root)
            path = root / "trace.json"
            captured = {}
            def factory(space, ledger):
                if ledger is None:
                    snap = root / "trace.optimizer-stopped.json"
                    captured["hash"] = digest(snap)
                    captured["trace"] = json.loads(path.read_text())
                    self.assertTrue(captured["trace"]["optimizer_completed"])
                    self.assertEqual(captured["trace"]["diagnostic_status"], "pending")
                return FakeEvaluator(clock)
            trace = runner.run_one(session, "slsqp_l0", "a", "theta0", 1, path,
                                   clock=clock, evaluator_factory=factory)
            self.assertEqual(trace["diagnostic_status"], "ok")
            self.assertEqual(digest(root / "trace.optimizer-stopped.json"), captured["hash"])
            for key in ("ledger", "validations", "attainment", "checkpoint_60", "stop_reason"):
                self.assertEqual(trace[key], captured["trace"][key], key)
            before = path.read_bytes()
            with self.assertRaises(FileExistsError):
                runner.run_one(session, "slsqp_l0", "a", "theta0", 1, path,
                               clock=clock, evaluator_factory=factory)
            self.assertEqual(path.read_bytes(), before)

    def test_interrupted_optimizer_preserves_prefix_and_does_not_run_diagnostic(self):
        with tempfile.TemporaryDirectory() as td:
            root, clock = Path(td), FakeClock()
            session, ev = fixture(root), FakeEvaluator(clock)
            def factory(space, ledger):
                self.assertIsNotNone(ledger)
                return ev
            def stop(run, u):
                successful_prefix(run, u)
                clock.advance(.25)
                raise Interrupted(signal.SIGTERM)
            with mock.patch.object(runner, "execute", stop):
                t = runner.run_one(session, "slsqp_l0", "a", "theta0", 1, root / "trace.json",
                                   clock=clock, evaluator_factory=factory)
            self.assertEqual(t["stop_reason"], "interrupted")
            self.assertFalse(t["optimizer_completed"])
            self.assertEqual(len(t["validations"]), 1)
            self.assertEqual(t["diagnostic_status"], "not_requested")
            self.assertLess(t["ledger"]["total_fe"], session["budget_fe"])
            self.assertTrue(ev.closed)
            self.assertFalse(t["interruption"]["verified_external_interruption"])

    def test_scientific_error_retains_completed_validations_as_terminal_outcome(self):
        with tempfile.TemporaryDirectory() as td:
            root, clock = Path(td), FakeClock()
            session = fixture(root)
            def fail(run, u):
                successful_prefix(run, u)
                raise RuntimeError("solver fixture failure")
            with mock.patch.object(runner, "execute", fail):
                t = runner.run_one(session, "slsqp", "a", "theta0", 1, root / "trace.json",
                    clock=clock, evaluator_factory=lambda sp, led: FakeEvaluator(clock))
            self.assertEqual(t["stop_reason"], "error")
            self.assertTrue(t["optimizer_completed"])
            self.assertEqual(len(t["validations"]), 1)
            self.assertNotIn("interruption", t)
            self.assertIn("solver fixture failure", t["error"])

    def test_diagnostic_interrupt_does_not_replace_optimizer_outcome(self):
        for method in ("surrogate", "cobyqa_l0", "slsqp_l0"):
            with self.subTest(method=method), tempfile.TemporaryDirectory() as td:
                root, clock = Path(td), FakeClock()
                session, holders = fixture(root), []
                def factory(space, ledger):
                    ev = FakeEvaluator(clock)
                    holders.append(ev)
                    if ledger is None:
                        def request(*args, **kw):
                            raise KeyboardInterrupt()
                        ev.request = request
                    return ev
                t = runner.run_one(session, method, "a", "theta0", 1, root / "trace.json",
                                   clock=clock, evaluator_factory=factory)
                snap = json.loads((root / "trace.optimizer-stopped.json").read_text())
                self.assertEqual(t["diagnostic_status"], "interrupted")
                self.assertTrue(t["optimizer_completed"])
                self.assertNotEqual(t["stop_reason"], "interrupted")
                for key in ("ledger", "validations", "attainment", "checkpoint_60"):
                    self.assertEqual(t[key], snap[key])
                self.assertTrue(all(ev.closed for ev in holders))
                if method == "surrogate":
                    state = snap["diagnostic_state"]
                    self.assertGreater(len(state["points"]), 0)
                    self.assertIsNotNone(state["incumbent"])

    def test_snapshot_reconstructs_diagnostic_without_optimizer_or_ledger(self):
        for method in ("surrogate", "cobyqa_l0", "slsqp_l0"):
            with self.subTest(method=method), tempfile.TemporaryDirectory() as td:
                root, clock = Path(td), FakeClock()
                session = fixture(root)
                trace = runner.run_one(session, method, "a", "theta0", 1, root / "trace.json",
                    clock=clock, evaluator_factory=lambda sp, led: FakeEvaluator(clock))
                snapfile = root / "trace.optimizer-stopped.json"
                before = snapfile.read_bytes()
                snap = json.loads(before)
                restored = runner.reconstruct_diagnostic_run(snap)
                with mock.patch.object(runner, "execute", side_effect=AssertionError("optimizer rerun")):
                    fn = (runner.post_run_diagnostic if method == "surrogate"
                          else runner.physical_post_run_diagnostic)
                    repaired = fn(restored, lambda: FakeEvaluator(clock), 1., clock)
                self.assertEqual(repaired["status"], "ok")
                for key in ("rows", "prediction" if method == "surrogate" else "selection"):
                    self.assertEqual(repaired[key], trace["post_run_diagnostic"][key])
                self.assertEqual(snapfile.read_bytes(), before)
                bad = copy.deepcopy(snap)
                bad["optimizer_completed"] = False
                with self.assertRaises(ValueError):
                    runner.reconstruct_diagnostic_run(bad)
                if method != "surrogate":
                    bad = copy.deepcopy(snap)
                    bad["diagnostic_state"]["selection"]["x"][0] += .1
                    with self.assertRaises(ValueError):
                        runner.reconstruct_diagnostic_run(bad)

    def test_signal_during_evidence_phase_is_deferred_and_handler_is_restored(self):
        old = signal.getsignal(signal.SIGTERM)
        with StopSignals() as signals:
            os.kill(os.getpid(), signal.SIGTERM)
            self.assertEqual(signals.requested.signum, signal.SIGTERM)
            with self.assertRaises(Interrupted):
                with signals.phase():
                    self.fail("pending interruption must prevent computation")
            # A second signal must not tear evidence writes/cleanup.
            os.kill(os.getpid(), signal.SIGTERM)
        self.assertEqual(signal.getsignal(signal.SIGTERM), old)

    def test_real_signals_stop_owned_evaluator_and_its_child(self):
        # A real Python run process owns a real JSON evaluator, which owns a
        # sleeping child. No compiler or FEM workload; external signals only.
        for stage, sig in (("optimizer", signal.SIGTERM), ("diagnostic", signal.SIGINT),
                           ("startup", signal.SIGTERM)):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                server = root / "server.py"
                server.write_text('''import json, os, pathlib, subprocess, sys, time
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
root = pathlib.Path(sys.argv[1])
(root / "owned.json").write_text(json.dumps([os.getpid(), child.pid]))
if sys.argv[2] == "startup":
    (root / "waiting").touch()
    time.sleep(60)
print(json.dumps({"op":"ready", "ok":True}), flush=True)
for line in sys.stdin:
    msg=json.loads(line)
    if msg["op"] == "wait":
        (root / "waiting").touch()
        time.sleep(60)
''')
                script = root / "worker.py"
                script.write_text('''import json, pathlib, sys
sys.path.insert(0, sys.argv[1])
import run as runner
from evaluator import Evaluator
from interruption import StopSignals
from test_run_interruption import fixture, successful_prefix
from test_harness import FakeEvaluator, FakeClock
root=pathlib.Path(sys.argv[2]); stage=sys.argv[3]
clock=FakeClock(); session=fixture(root)
class Bridge(FakeEvaluator):
    def __init__(self):
        super().__init__(clock)
        self.real=Evaluator([sys.executable, str(root/"server.py"), str(root), stage], timeout=30)
    def close(self):
        self.real.close(); super().close()
    def kill(self):
        self.real.kill()
    def resources(self):
        return self.real.resources()
    def request(self, op, **kw):
        if op == "diagnose":
            return self.real.request("wait", timeout=30)
        return super().request(op, **kw)
def factory(space, ledger):
    if stage == "diagnostic" and ledger is not None:
        return FakeEvaluator(clock)
    return Bridge()
def execute(run, u):
    result=successful_prefix(run,u)
    if stage == "optimizer":
        run.ev.real.request("wait",timeout=30)
    return result
runner.execute=execute
with StopSignals() as signals:
    t=runner.run_one(session,"slsqp_l0","a","theta0",1,root/"trace.json",
                     clock=clock,evaluator_factory=factory,signals=signals)
sys.exit(128+t["process_interruption"]["signal"])
''')
                proc = subprocess.Popen([sys.executable, str(script), str(HERE), str(root), stage],
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
                    os.kill(proc.pid, sig)
                    stdout, stderr = proc.communicate(timeout=10)
                    self.assertEqual(proc.returncode, 128 + sig, stderr.decode())
                    t = json.loads((root / "trace.json").read_text())
                    self.assertEqual(t["optimizer_completed"], stage == "diagnostic")
                    if stage != "startup":
                        self.assertEqual(len(t["validations"]), 1)
                    if stage == "diagnostic":
                        self.assertEqual(t["diagnostic_status"], "interrupted")
                    phase = t.get("post_run_diagnostic", {}) if stage == "diagnostic" else t
                    receipt = phase["resources"]["evaluator"]["child_receipt"]
                    self.assertEqual(receipt["state"], "reaped")
                    self.assertFalse(receipt["cpu_complete"])
                    # Zombies are terminated, not executing, and may briefly await
                    # their OS reaper. No owned PID may remain runnable/sleeping.
                    for pid in owned:
                        state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)],
                                               capture_output=True, text=True).stdout.strip()
                        self.assertTrue(not state or state.startswith("Z"), (pid, state))
                finally:
                    if proc.poll() is None:
                        proc.kill(); proc.communicate()
                    if owned:
                        try:
                            os.killpg(owned[0], signal.SIGKILL)
                        except ProcessLookupError:
                            pass


if __name__ == "__main__":
    unittest.main(verbosity=2)
