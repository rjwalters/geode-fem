"""Controlled end-to-end recovery with real launcher/run/evaluator processes.

The numerical model is a fixture; these tests are not FEM timing evidence.
Only the runtime fingerprint and host-load adapters are synthetic. Registry,
process identities, immutable files, policy, dispatch and child cleanup are real.
"""
import argparse
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
import recovery
import recovery_bindings
import session
from session_journal import Journal, digest, freeze, read_events


def probes():
    return {"source_probe": lambda repo: {"fixture": "sources", "tree_clean": True},
            "runtime_probe": lambda repo: {"fixture": "runtime"}}


def proof(root, key):
    path = root / "operator-stop.txt"
    if not path.exists(): path.write_text("controlled external-stop fixture; no scientific experiment")
    return {"attempt": key, "verified_external_interruption": True, "event_kind": "operator_stop",
            "reason": "controlled fixture", "verified_by": "fixture reviewer", "verification_reference": "fixture review",
            "event_id": key, "evidence": [{"path": str(path), "sha256": digest(path)}]}


def initialize(root, variant):
    from test_run_interruption import fixture
    from test_harness import FakeEvaluator, FakeClock
    import run as runner
    from interruption import Interrupted
    root = root.resolve()
    journal = Journal(root, {"fixture": True})
    config = fixture(root)
    config.update(label="smoke", procs=2 if variant == "parallel" else 1, fixture_variant=variant)
    for level in (0, 1, 2):
        (root / f"L{level}.msh").write_text(f"fixture mesh {level}")
        if level in (0, 1): config["models"][f"L{level}"]["train_mesh"] = str(root / f"L{level}.msh")
    config["evaluator"].update(binary=sys.executable, val_mesh=str(root / "L2.msh"), val_level=2)
    jobs = [{"name": f"j{k}", "position": k, "method": "slsqp_l0" if k == 5 else "slsqp",
             "space": "a", "start": "theta0", "repeat": 1} for k in range(6)]
    freeze(root / "session.json", config)
    freeze(root / "queue.json", {"jobs": jobs})
    freeze(root / "calibration_primary.json", {"accepted_for_dispatch": True, "calibration": {"unit_seconds": 1.}})
    freeze(root / "recovery-inputs.json", recovery_bindings.capture(config, **probes()))
    names = ("session.json", "queue.json", "calibration_primary.json", "recovery-inputs.json")
    freeze(root / "bindings.json", {"files": {name: digest(root / name) for name in names},
        "procs": config["procs"], "unit_seconds": 1., "session_directory": str(root),
        "recovery_registry": str(root.with_name(root.name + ".recoveries")),
        "dispatch_settings": {"gate": True, "max_wait": 0., "interval": .01, "max_deferrals": 2}})
    journal.emit("session_ready", bindings_sha256=digest(root / "bindings.json"))
    external = {}
    for k, job in enumerate(jobs):
        if k == 3 and variant != "diagnostic_only": continue
        aid = journal.emit("attempt_created", job=job, suffix="")
        if k == 1 and variant != "diagnostic_only":
            journal.emit("attempt_finished", attempt_id=aid, result={"status": "deferred"})
            continue
        trace = root / f"j{k}.json"
        journal.emit("attempt_dispatched", attempt_id=aid, trace=trace.name)
        if k == 2 and variant != "diagnostic_only":
            key = f"{root}#{aid}"; external[key] = proof(root, key)
            continue
        if k == 5:
            clock = FakeClock()
            def factory(space, ledger):
                if ledger is None: raise Interrupted(signal.SIGTERM)
                return FakeEvaluator(clock)
            runner.run_one(config, "slsqp_l0", "a", "theta0", 1, trace,
                           evaluator_factory=factory, clock=clock)
            code = 143
        else:
            freeze(trace, dict(job, label="smoke", session=config["session_id"], unit_seconds=1.,
                budget_fe=config["budget_fe"], optimizer_completed=True, diagnostic_status="not_requested",
                stop_reason="error" if k == 0 else "converged", ledger={"total_fe": .1, "total_seconds": .1}))
            code = 1 if k == 0 else 0
        journal.emit("attempt_finished", attempt_id=aid, result={"status": "ran", "exit": code,
            "trace": trace.name, "trace_sha256": digest(trace),
            "host_contaminated": k == 4 and variant != "diagnostic_only"})
    if variant == "orphan":
        import process_tracking
        command = [sys.executable, "-c", "import time; time.sleep(60)"]
        tracker = process_tracking.Spawn(journal, command, role="direct")
        proc = subprocess.Popen(command, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        tracker.started(proc.pid)
        freeze(root / "orphan.json", {"pid": proc.pid})
    freeze(root / "external-events.json", external)
    journal.close()


def optimizer(argv):
    from interruption import StopSignals, Interrupted
    from evaluator import Evaluator
    ap = argparse.ArgumentParser()
    for name in ("session", "method", "space", "start", "repeat", "position", "out"):
        ap.add_argument("--" + name, required=True)
    args = ap.parse_args(argv)
    config = json.loads(Path(args.session).read_text()); out = Path(args.out)
    journal = Journal(out.with_name(out.stem + ".processes"), {"fixture": True})
    completed, code, reason, ev = True, 0, "converged", None
    started = time.time()
    with StopSignals() as signals:
        try:
            with signals.phase():
                if config["fixture_variant"] == "interrupt" and args.position == "3":
                    server = '''import json,os,pathlib,subprocess,sys,time
root=pathlib.Path(sys.argv[1])
child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"])
(root/"owned.json").write_text(json.dumps([os.getpid(),child.pid]))
print(json.dumps({"op":"ready","ok":True}),flush=True)
for line in sys.stdin:
 (root/"waiting").touch(); time.sleep(60)
'''
                    ev = Evaluator([sys.executable, "-c", server, str(out.parent)], timeout=30, process_journal=journal)
                    ev.request("wait", timeout=30)
                if config["fixture_variant"] == "replace_stop" and args.position == "2":
                    raise Interrupted(signal.SIGTERM)
        except Interrupted as error:
            completed, code, reason = False, 128 + error.signum, "interrupted"
        finally:
            if ev is not None:
                if not completed: ev.kill()
                ev.close()
            journal.close()
    elapsed = time.time() - started
    freeze(out, {"method": args.method, "space": args.space, "start": args.start, "repeat": int(args.repeat),
        "label": config["label"], "session": config["session_id"], "unit_seconds": config["unit_seconds"],
        "budget_fe": config["budget_fe"], "optimizer_completed": completed, "stop_reason": reason,
        "diagnostic_status": "not_requested", "in_run_start_time": started, "in_run_end_time": time.time(),
        "ledger": {"total_seconds": elapsed, "total_fe": elapsed / config["unit_seconds"]},
        "resources": {"cpu_complete": False, "fixture": True}, "validations": [], "attainment": {}})
    return code


def diagnostic(argv):
    import diagnostic_repair
    import run as runner
    from test_harness import FakeClock, FakeEvaluator
    ap = argparse.ArgumentParser()
    for name in ("session", "snapshot", "snapshot-sha256", "out", "attempt-id"):
        ap.add_argument("--" + name, required=True)
    args = ap.parse_args(argv)
    with mock.patch.object(runner, "execute", side_effect=AssertionError("optimizer repeated")), \
         mock.patch.object(runner, "Ledger", side_effect=AssertionError("optimizer ledger created")):
        record = diagnostic_repair.execute(args.snapshot, args.snapshot_sha256,
            json.loads(Path(args.session).read_text()), args.out, attempt_id=args.attempt_id,
            evaluator_factory=lambda journal: FakeEvaluator(FakeClock()))
    return 0 if record["status"] == "ok" else 1


def launch(root, events_file=None):
    from interruption import StopSignals
    config = json.loads((root / "session.json").read_text())
    calls = 0
    def verify(expected, observed):
        nonlocal calls
        calls += 1
        if config["fixture_variant"] == "verify_false":
            return {"verified": False}
        if config["fixture_variant"] == "drift" and calls >= 2:
            raise ValueError("fixture runtime changed at dispatch")
        return recovery_bindings.verify(expected, observed, **probes())
    def command(mode):
        def replace(argv): return [*argv[:2], str(Path(__file__).resolve()), "--fixture-" + mode, *argv[3:]]
        return replace
    waits = 0
    def gate(*args, **kwargs):
        nonlocal waits
        waits += 1
        return (not (config["fixture_variant"] == "diagnostic_only" and waits == 1), 0., 0.)
    events = json.loads((events_file or root / "external-events.json").read_text())
    with StopSignals() as signals, mock.patch.object(session, "calibrate", side_effect=AssertionError("unit remeasured")):
        result = recovery.recover(root, "smoke", external_events=events, stop=lambda: signals.requested,
            binding_verifier=verify, monitor_factory=lambda **kw: session.LoadMonitor(load=lambda: (0., 0., 0.), **kw),
            wait=gate, optimizer_command=command("optimizer"), diagnostic_command=command("diagnostic"))
    print(json.dumps(result))
    return 128 + signals.requested.signum if signals.requested else (0 if result["complete"] else 1)


class ExecutionTests(unittest.TestCase):
    def setup_source(self, td, variant="normal"):
        root = Path(td) / "source"
        result = subprocess.run([sys.executable, "-I", str(Path(__file__).resolve()), "--fixture-init", str(root), variant],
                                capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        return root.resolve()

    def invoke(self, root, events=None):
        argv = [sys.executable, "-I", str(Path(__file__).resolve()), "--fixture-launch", str(root)]
        if events: argv.append(str(events))
        return subprocess.run(argv, capture_output=True, text=True, timeout=30)

    def original_bytes(self, root):
        return {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}

    def test_complete_linked_recovery_preserves_source_order_unit_and_diagnostic(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td); before = self.original_bytes(root)
            result = self.invoke(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(result.stdout)
            self.assertTrue(summary["complete"])
            self.assertEqual(summary["unit_seconds"], 1.)
            self.assertEqual(summary["procs"], 1)
            self.assertGreater(summary["recovery_wall_seconds"], summary["preparation_wall_seconds"])
            self.assertGreater(summary["verification_wall_seconds"], 0.)
            generation = Path(summary["directory"])
            events = read_events(generation / "events.jsonl")["events"]
            dispatched = [e for e in events if e["kind"] == "attempt_dispatched"]
            names = [e["trace"] for e in dispatched]
            self.assertEqual(names, ["j3.json", "j1.json", "j2.json", "j4.json"])
            self.assertEqual(sum(e["kind"] == "recovery_dispatch_verified" for e in events), 5)
            self.assertTrue(all(row["launcher_receipt"]["state"] == "reaped" for row in summary["optimizer_results"]))
            self.assertEqual(summary["policy"]["jobs"][0]["selected_completion"]["source"], str(root))
            self.assertEqual(summary["policy"]["jobs"][5]["selected_diagnostic"]["status"], "ok")
            self.assertEqual(summary["policy"]["replacement_job_count"], 2)
            for path, raw in before.items(): self.assertEqual(path.read_bytes(), raw, path)
            # A second invocation uses the existing chain; no optimizer is repeated.
            again = self.invoke(root)
            self.assertEqual(again.returncode, 0, again.stderr)
            self.assertIsNone(json.loads(again.stdout)["generation"])

    def test_deferred_diagnostic_has_distinct_attempt_and_no_optimizer_dispatch(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "diagnostic_only")
            result = self.invoke(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(result.stdout)
            events = read_events(Path(summary["directory"]) / "events.jsonl")["events"]
            self.assertFalse(any(e["kind"] == "attempt_dispatched" for e in events))
            created = [e for e in events if e["kind"] == "diagnostic_attempt_created"]
            self.assertEqual(len(created), 2)
            self.assertNotEqual(created[0]["directory"], created[1]["directory"])
            self.assertEqual(len(summary["policy"]["jobs"][5]["diagnostic_attempts"]), 2)

    def test_parallel_recovery_keeps_worker_count_and_distinct_lifetime_receipts(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "parallel")
            result = self.invoke(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(result.stdout)
            self.assertEqual(summary["procs"], 2)
            receipts = [row["launcher_receipt"] for row in summary["optimizer_results"]]
            self.assertEqual(len({receipt["pid"] for receipt in receipts}), 4)
            self.assertTrue(all(receipt["state"] == "reaped" and receipt["available"] for receipt in receipts))
            events = read_events(Path(summary["directory"]) / "events.jsonl")["events"]
            phase = next(e["sequence"] for e in events if e["kind"] == "recovery_phase_finished"
                         and e["phase"] == "remaining_registered")
            for event in events:
                if event["kind"] == "attempt_dispatched" and event["trace"] != "j3.json":
                    self.assertGreater(event["sequence"], phase)

    def test_fresh_dispatch_verification_failure_launches_nothing(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "drift")
            result = self.invoke(root)
            self.assertEqual(result.returncode, 1, result.stderr)
            summary = json.loads(result.stdout)
            self.assertFalse(summary["complete"])
            self.assertIn("fixture runtime changed", summary["verification_failures"][0])
            events = read_events(Path(summary["directory"]) / "events.jsonl")["events"]
            self.assertFalse(any(e["kind"] == "process_spawn_started" for e in events))
            again = self.invoke(root)
            self.assertNotEqual(again.returncode, 0)
            self.assertIn("requires disposition", again.stderr)

    def test_real_orphan_blocks_reservation_and_launch(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "orphan")
            pid = json.loads((root / "orphan.json").read_text())["pid"]
            try:
                result = self.invoke(root)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("not verified stopped", result.stderr)
                self.assertEqual(list(root.with_name("source.recoveries").glob("generation-*")), [])
            finally:
                try: os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError: pass
                deadline = time.monotonic() + 3
                while True:
                    state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)],
                                           capture_output=True, text=True).stdout.strip()
                    if not state or state.startswith("Z"): break
                    self.assertLess(time.monotonic(), deadline, "test-owned orphan did not terminate")
                    time.sleep(.01)

    def test_negative_binding_verdict_cannot_be_treated_as_permission(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "verify_false")
            result = self.invoke(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("bindings were not verified", result.stderr)
            self.assertEqual(list(root.with_name("source.recoveries").glob("generation-*")), [])

    def test_changed_diagnostic_inputs_block_later_recovery(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td)
            first = self.invoke(root)
            self.assertEqual(first.returncode, 0, first.stderr)
            summary = json.loads(first.stdout)
            terminal = Path(summary["policy"]["jobs"][5]["selected_diagnostic"]["terminal"]["file"])
            inputs = terminal.parent / "inputs.json"
            raw = json.loads(inputs.read_text()); raw["session"]["unit_seconds"] *= 2
            inputs.write_text(json.dumps(raw))
            again = self.invoke(root)
            self.assertNotEqual(again.returncode, 0)
            self.assertIn("binding", again.stderr)
            self.assertEqual(len(list(root.with_name("source.recoveries").glob("generation-*"))), 1)

    def test_second_external_replacement_remains_forbidden_with_new_reviewed_evidence(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "replace_stop")
            first = self.invoke(root)
            self.assertEqual(first.returncode, 1, first.stderr)
            summary = json.loads(first.stdout)
            row = summary["policy"]["jobs"][2]
            self.assertEqual(row["external_replacements_used"], 1)
            latest = row["attempts"][-1]["key"]
            event_file = Path(td) / "reviewed-second-stop.json"
            event_file.write_text(json.dumps({latest: proof(Path(td), latest)}))
            second = self.invoke(root, event_file)
            self.assertEqual(second.returncode, 1, second.stderr)
            self.assertIsNone(json.loads(second.stdout)["generation"])
            self.assertEqual(len(list(root.with_name("source.recoveries").glob("generation-*"))), 1)

    def test_real_launcher_signal_cleans_full_chain_and_leaves_tail_unlaunched(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.setup_source(td, "interrupt")
            proc = subprocess.Popen([sys.executable, "-I", str(Path(__file__).resolve()), "--fixture-launch", str(root)],
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            generation = root.with_name("source.recoveries") / "generation-0001"
            owned = []
            try:
                deadline = time.monotonic() + 15
                while not (generation / "waiting").exists() and time.monotonic() < deadline:
                    if proc.poll() is not None: self.fail(proc.communicate()[1])
                    time.sleep(.01)
                self.assertTrue((generation / "waiting").exists())
                owned = json.loads((generation / "owned.json").read_text())
                second = self.invoke(root)
                self.assertNotEqual(second.returncode, 0)
                self.assertIn("BlockingIOError", second.stderr)
                proc.send_signal(signal.SIGTERM)
                stdout, stderr = proc.communicate(timeout=15)
                self.assertEqual(proc.returncode, 143, stderr)
                summary = json.loads(stdout)
                self.assertFalse(summary["complete"])
                events = read_events(generation / "events.jsonl")["events"]
                self.assertEqual([e["trace"] for e in events if e["kind"] == "attempt_dispatched"], ["j3.json"])
                import process_verification
                verified = process_verification.verify(generation)
                self.assertTrue(verified["processes_stopped"], verified["issues"])
            finally:
                if proc.poll() is None: proc.kill(); proc.communicate()
                if owned:
                    try: os.killpg(owned[0], signal.SIGKILL)
                    except ProcessLookupError: pass


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1].startswith("--fixture-"):
        mode = sys.argv.pop(1)
        if mode == "--fixture-init": initialize(Path(sys.argv[1]), sys.argv[2]); sys.exit(0)
        if mode == "--fixture-optimizer": sys.exit(optimizer(sys.argv[1:]))
        if mode == "--fixture-diagnostic": sys.exit(diagnostic(sys.argv[1:]))
        if mode == "--fixture-launch": sys.exit(launch(Path(sys.argv[1]), Path(sys.argv[2]) if len(sys.argv) > 2 else None))
        raise ValueError(mode)
    unittest.main(verbosity=2)
