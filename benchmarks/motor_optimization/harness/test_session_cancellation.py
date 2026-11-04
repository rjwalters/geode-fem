"""Real launcher receipts and cooperative cancellation; no FEM/main data."""
import concurrent.futures
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
from unittest import mock
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import owned_child
import session
from interruption import Interrupted
from session_journal import Journal, read_events


class CancellationTests(unittest.TestCase):
    def test_concurrent_lifetimes_have_separate_exclusive_wait4_receipts(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); journal = Journal(root / "journal", {})
            popen = subprocess.Popen
            class SoleWait4(popen):
                def poll(self): raise AssertionError("Popen.poll consumed a wait4 receipt")
                def wait(self, *args, **kw): raise AssertionError("Popen.wait consumed a wait4 receipt")
            def launch(k):
                cmd = [sys.executable, "-c",
                       "import time; end=time.process_time()+.06\nwhile time.process_time()<end: pass"]
                with (root / f"child{k}.log").open("x") as log:
                    return owned_child.wait_run(cmd, log=log, env=os.environ.copy(),
                                                journal=journal, attempt_id=k)
            with mock.patch.object(owned_child.subprocess, "Popen", SoleWait4), \
                 concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                receipts = list(pool.map(launch, (0, 1)))
            journal.close()
            self.assertEqual(len({r["pid"] for r in receipts}), 2)
            for r in receipts:
                self.assertEqual(r["state"], "reaped")
                self.assertTrue(r["available"])
                self.assertEqual(r["returncode"], 0)
                self.assertGreaterEqual(r["cpu"]["total_seconds"], .05)
                self.assertFalse(r["cpu_complete"])
                with self.assertRaises(ChildProcessError): os.waitpid(r["pid"], os.WNOHANG)
            kinds = [e["kind"] for e in read_events(root / "journal/events.jsonl")["events"]]
            self.assertEqual(kinds.count("owned_process_started"), 2)
            self.assertEqual(kinds.count("owned_process_reaped"), 2)

    def test_forced_exit_is_reaped_but_never_complete(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); ready = root / "ready"
            code = ("import pathlib,signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                    f"pathlib.Path({str(ready)!r}).touch(); time.sleep(60)")
            with (root / "child.log").open("x") as log:
                receipt = owned_child.wait_run([sys.executable, "-c", code], log=log,
                    env=os.environ.copy(), stop=lambda: Interrupted(15) if ready.exists() else None,
                    grace_seconds=.1, poll_seconds=.01)
            self.assertEqual(receipt["returncode"], -signal.SIGKILL)
            self.assertTrue(receipt["forced_kill"])
            self.assertFalse(receipt["cpu_complete"])
            self.assertFalse(receipt["descendant_cleanup_verified"])
            self.assertEqual([s["signal"] for s in receipt["signals"]], [signal.SIGTERM, signal.SIGKILL])

    def test_failed_journal_does_not_abandon_child(self):
        class BrokenJournal:
            def emit(self, kind, **kw):
                if kind == "process_spawn_intent": return 0
                raise OSError("fixture journal failure")
        with tempfile.TemporaryDirectory() as td, (Path(td) / "log").open("x") as log:
            with self.assertRaisesRegex(OSError, "fixture journal") as raised:
                owned_child.wait_run([sys.executable, "-c", "import time; time.sleep(60)"],
                                     log=log, env=os.environ.copy(), journal=BrokenJournal(),
                                     grace_seconds=.1, poll_seconds=.01)
            receipt = raised.exception.launcher_receipt
            self.assertEqual(receipt["state"], "reaped")
            with self.assertRaises(ChildProcessError): os.waitpid(receipt["pid"], os.WNOHANG)

    def test_completeness_requires_terminal_phase_receipts(self):
        receipt = {"available": True, "returncode": 0, "forced_kill": False}
        trace = {"optimizer_completed": True, "diagnostic_status": "not_requested",
                 "resources": {"cpu_complete": True}}
        self.assertTrue(owned_child.bind_completeness(receipt, trace)["cpu_complete"])
        for change in ({"resources": {}}, {"diagnostic_status": "pending"},
                       {"diagnostic_status": None}, {"optimizer_completed": False},
                       {"post_run_diagnostic": {"status": "interrupted"}}):
            self.assertFalse(owned_child.bind_completeness(receipt, dict(trace, **change))["cpu_complete"])
        diag = dict(trace, diagnostic_status="ok", post_run_diagnostic={"status": "ok", "resources": {"cpu_complete": True}})
        self.assertTrue(owned_child.bind_completeness(receipt, diag)["cpu_complete"])
        self.assertFalse(owned_child.bind_completeness(dict(receipt, returncode=143), diag)["cpu_complete"])

    def test_cancelled_evaluator_never_spawns(self):
        from evaluator import Evaluator
        with mock.patch("evaluator.subprocess.Popen", side_effect=AssertionError("cancelled evaluator spawned")):
            with self.assertRaises(Interrupted):
                Evaluator(["unused"], stop=lambda: Interrupted(signal.SIGTERM))

    def test_cancelled_host_gate_does_not_wait_or_dispatch(self):
        stop = lambda: Interrupted(signal.SIGINT)
        with self.assertRaises(Interrupted):
            session.wait_for_idle(1, 1000, interval=100, load=lambda: (100., 0., 0.), stop=stop,
                                  sleep=lambda n: self.fail("cancelled gate slept"))
        jobs = [{"name": "not_started", "position": 0}]
        result = session.schedule(jobs, {"procs": 2, "stop": stop}, 3,
                                  run=lambda *a: self.fail("cancelled queue dispatched"))
        self.assertEqual(result, [])

    def test_worker_start_failure_cancels_and_joins_already_started_workers(self):
        jobs = [{"name": f"job{k}", "position": k} for k in range(3)]
        entered, finished = threading.Event(), threading.Event()
        def work(job, ctx, suffix):
            entered.set()
            while ctx["stop"]() is None:
                time.sleep(.01)
            finished.set()
            raise Interrupted(2)
        original = threading.Thread.start
        starts = []
        def start(thread):
            starts.append(thread)
            if len(starts) == 2:
                self.assertTrue(entered.wait(2))
                raise RuntimeError("fixture thread-start failure")
            original(thread)
        with mock.patch.object(threading.Thread, "start", start):
            with self.assertRaisesRegex(RuntimeError, "thread-start failure"):
                session.schedule(jobs, {"procs": 2}, 1, run=work)
        self.assertTrue(finished.is_set())
        self.assertFalse(starts[0].is_alive())

    def test_space_b_calibration_startup_failure_is_archived(self):
        with tempfile.TemporaryDirectory() as td:
            root, instances = Path(td), []
            class Fake:
                def __init__(self, argv, **kwargs):
                    if argv[argv.index("--space") + 1] == "b":
                        error = RuntimeError("space B startup fixture")
                        error.evaluator_resources = {"marker": "retained B startup receipt"}
                        raise error
                    self.closed = False
                    instances.append(self)
                def request(self, *args, **kwargs):
                    return {"ok": True, "unit_seconds": 1., "theta0_train": {"F": 1., "mean_full": 2.},
                        "theta0_validation": {}, "validation_seconds": [1., 1., 1.],
                        "train_forward_seconds": [.2], "train_adjoint_seconds": [.1],
                        "weights": {"validation_solves_per_eval": 60, "train_solves_per_eval": 60},
                        "deterministic": True, "anchor_agrees_bitwise": True, "anchor_agrees_4ulp": True}
                def close(self): self.closed = True
                def resources(self): return {"child_receipt": {"cpu_complete": True}}
            with mock.patch.object(session, "Evaluator", Fake):
                with self.assertRaisesRegex(RuntimeError, "space B startup"):
                    session.calibrate("fixture", "train", "validation", 0, 0, None, root, 2)
            self.assertEqual(len(instances), 3)
            self.assertTrue(all(ev.closed for ev in instances))
            record = json.loads((root / "calibration_request_b0.json").read_text())
            self.assertIn("space B startup fixture", record["error"])
            self.assertEqual(record["process_resources"]["marker"], "retained B startup receipt")

    def test_real_session_signal_stops_run_tree_and_preserves_undispatched_queue(self):
        self.signal_fixture("dispatch", signal.SIGTERM)

    def test_real_session_signal_stops_all_concurrent_calibration_workers(self):
        self.signal_fixture("calibration", signal.SIGINT)

    def signal_fixture(self, mode, sig):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            server = root / "fake-evaluator"
            server.write_text(f"#!{sys.executable}\n" + '''import json,os,pathlib,subprocess,sys,time
sys.path.insert(0, ''' + repr(str(HERE)) + ''')
from test_harness import FakeEvaluator, FakeClock
opts=dict(zip(sys.argv[1::2],sys.argv[2::2]))
out=pathlib.Path(opts["--mesh-out"]); out.mkdir(parents=True,exist_ok=True)
mode=''' + repr(mode) + '''
fake=FakeEvaluator(FakeClock())
print(json.dumps(fake.ready),flush=True)
for line in sys.stdin:
    req=json.loads(line)
    if req["op"] == "shutdown":
        print(json.dumps({"op":"shutdown","ok":True,"id":req["id"]}),flush=True)
        break
    if req["op"] == "calibrate_unit" and "single0" in str(out):
        print(json.dumps({"ok":True,"id":req["id"],"unit_seconds":1.,"validation_seconds":[1.,1.,1.],"weights":{}}),flush=True)
        continue
    child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"])
    (out/"owned.json").write_text(json.dumps([os.getpid(),child.pid]))
    (out/"waiting").touch()
    time.sleep(60)
''')
            server.chmod(0o755)
            script = root / "launcher.py"
            script.write_text('''import json,pathlib,sys
sys.path.insert(0,sys.argv[1])
import session
from session_journal import Journal,freeze
from test_run_interruption import fixture
from types import SimpleNamespace
root=pathlib.Path(sys.argv[2]); mode=sys.argv[3]
def controlled(argv=None,stop=None):
    if mode == "calibration":
        out=root/"calibration"; out.mkdir()
        session.calibrate(root/"fake-evaluator",root/"unused",root/"unused",0,0,None,out,2,stop=stop)
        raise AssertionError("calibration unexpectedly finished")
    out=root/"session"; journal=Journal(out,{"label":"test"}); (out/"logs").mkdir()
    configuration=fixture(root)
    configuration["evaluator"].update(binary=str(root/"fake-evaluator"),val_mesh="fixture",val_level=0)
    freeze(out/"session.json",configuration)
    jobs=[dict(name=f"job{k}",method="slsqp_l0",space="a",start="theta0",repeat=1,position=k) for k in range(3)]
    freeze(out/"queue.json",{"jobs":jobs})
    ctx=dict(out=out,procs=1,gate=False,max_wait=0,interval=.01,journal=journal,stop=stop,
             monitor=SimpleNamespace(samples=[],error=None))
    try:
        result=session.schedule(jobs,ctx,2)
        freeze(out/"results.json",result)
        session.interruption.check(stop)
    finally:
        journal.close()
session._main=controlled
sys.exit(session.main())
''')
            proc = subprocess.Popen([sys.executable, "-I", str(script), str(HERE), str(root), mode],
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            owned = []
            try:
                deadline = time.monotonic() + 20
                expected = 2 if mode == "calibration" else 1
                while len(list(root.rglob("waiting"))) != expected and time.monotonic() < deadline:
                    if proc.poll() is not None:
                        self.fail(proc.communicate()[1].decode())
                    time.sleep(.02)
                self.assertEqual(len(list(root.rglob("waiting"))), expected)
                owned = [json.loads(p.read_text()) for p in root.rglob("owned.json")]
                os.kill(proc.pid, sig)
                stdout, stderr = proc.communicate(timeout=15)
                self.assertEqual(proc.returncode, 128 + sig, stderr.decode())
                if mode == "dispatch":
                    events = read_events(root / "session/events.jsonl")["events"]
                    self.assertEqual(sum(e["kind"] == "attempt_dispatched" for e in events), 1)
                    self.assertEqual(sum(e["kind"] == "owned_process_reaped" for e in events), 1)
                    self.assertEqual(events[-1]["kind"], "attempt_finished")
                    results = json.loads((root / "session/results.json").read_text())
                    self.assertEqual(len(results), 1)
                    self.assertEqual(results[0]["stop_reason"], "interrupted")
                    self.assertTrue(results[0]["launcher_receipt"]["available"])
                    self.assertFalse(results[0]["launcher_receipt"]["cpu_complete"])
                    self.assertFalse((root / "session/job1.json").exists())
                    self.assertFalse((root / "session/job2.json").exists())
                else:
                    records = list((root / "calibration").glob("calibration_request_concurrent*.json"))
                    self.assertEqual(len(records), 2)
                    for record in records:
                        raw = json.loads(record.read_text())
                        self.assertIn("Interrupted", raw["error"])
                        self.assertEqual(raw["process_resources"]["child_receipt"]["state"], "reaped")
                for pair in owned:
                    for pid in pair:
                        state = subprocess.run(["ps","-o","stat=","-p",str(pid)],capture_output=True,text=True).stdout.strip()
                        self.assertTrue(not state or state.startswith("Z"), (pid,state))
            finally:
                if proc.poll() is None:
                    proc.kill(); proc.communicate()
                # Only fixture-created groups; catch late-start records too.
                for record in root.rglob("owned.json"):
                    pid = json.loads(record.read_text())[0]
                    try: os.killpg(pid,signal.SIGKILL)
                    except ProcessLookupError: pass


if __name__ == "__main__":
    unittest.main(verbosity=2)
