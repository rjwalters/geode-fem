"""Process recovery checks refuse live groups, PID reuse and unresolved fork gaps."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import process_identity
import process_verification
from session_journal import Journal, read_events


class VerificationTests(unittest.TestCase):
    def fake_identity(self, pid=999999):
        return {"pid": pid, "pgid": pid, "ppid": 1, "start": "Sun Oct 18 00:00:00 2026",
                "state": "S", "host": process_identity.host()}

    def synthetic(self, root, *, gap=False, role="evaluator", child=None):
        journal = Journal(root, {})
        index = journal.emit("process_spawn_intent", role=role, command=["fixture"],
                             child_journal=child)
        if not gap:
            journal.emit("process_spawn_started", spawn_id=index, identity=self.fake_identity())
        journal.close()

    def test_empty_groups_prove_only_time_bound_absence(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; self.synthetic(root)
            result = process_verification.verify(root, observe=lambda: [])
            self.assertTrue(result["processes_stopped"], result["issues"])
            self.assertFalse(result["can_launch"])
            self.assertIn(".", result["journals"])

    def test_live_group_without_its_original_leader_blocks_recovery(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; self.synthetic(root)
            member = dict(self.fake_identity(999998), pgid=999999)
            result = process_verification.verify(root, observe=lambda: [member])
            self.assertFalse(result["processes_stopped"])
            self.assertTrue(any("group still" in s for s in result["issues"]))
            member["state"] = "Z"
            self.assertTrue(process_verification.verify(root, observe=lambda: [member])["processes_stopped"])

    def test_reused_pid_or_changed_boot_is_never_treated_as_safe_absence(self):
        for kind in ("pid", "boot"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"; self.synthetic(root)
                current = dict(self.fake_identity(), start="different start", pgid=77)
                result = process_verification.verify(root, observe=lambda: [current] if kind == "pid" else [],
                    host=(lambda: dict(process_identity.host(), boot="different boot")) if kind == "boot" else process_identity.host)
                self.assertFalse(result["processes_stopped"])
                self.assertTrue(any("differs" in s or "changed host" in s for s in result["issues"]))

    def test_fork_gap_torn_tail_and_missing_child_journal_are_unresolved(self):
        for kind in ("gap", "tail", "missing", "escape"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"
                self.synthetic(root, gap=kind == "gap", role="run" if kind in ("missing", "escape") else "evaluator",
                               child="../escape" if kind == "escape" else "job.processes" if kind == "missing" else None)
                if kind == "tail":
                    with (root / "events.jsonl").open("ab") as stream: stream.write(b'{"kind":')
                self.assertFalse(process_verification.verify(root, observe=lambda: [])["processes_stopped"])

    def test_live_owner_lock_blocks_even_an_empty_process_observation(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; journal = Journal(root, {})
            try:
                self.assertFalse(process_verification.verify(root, observe=lambda: [])["processes_stopped"])
            finally:
                journal.close()

    def test_malformed_identity_and_untracked_owned_process_are_not_absence_proof(self):
        for mode in ("identity", "untracked", "role"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"; journal = Journal(root, {})
                if mode == "untracked":
                    journal.emit("owned_process_started", pid=999999)
                else:
                    index = journal.emit("process_spawn_intent", role="unknown" if mode == "role" else "evaluator")
                    identity = self.fake_identity() if mode == "role" else {"host": process_identity.host()}
                    journal.emit("process_spawn_started", spawn_id=index, identity=identity)
                journal.close()
                self.assertFalse(process_verification.verify(root, observe=lambda: [])["processes_stopped"])

    def test_unavailable_observation_cannot_prove_cleanup(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; self.synthetic(root)
            def missing(): raise OSError("fixture ps failure")
            result = process_verification.verify(root, observe=missing)
            self.assertFalse(result["processes_stopped"])
            self.assertIn("unavailable", result["issues"][0])

    def test_normal_evaluator_and_failed_spawn_leave_complete_tracking(self):
        for mode in ("normal", "immediate", "failed"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                server = root / "server.py"
                server.write_text('''import json,sys
print(json.dumps({"op":"ready","ok":True}),flush=True)
for line in sys.stdin:
    r=json.loads(line)
    print(json.dumps({"op":"shutdown","ok":True,"id":r["id"]}),flush=True)
    break
''')
                worker = root / "worker.py"
                worker.write_text('''import pathlib,sys
sys.path.insert(0,sys.argv[1])
from evaluator import Evaluator
from session_journal import Journal
from owned_child import wait_run
import os
root=pathlib.Path(sys.argv[2]); j=Journal(root/"session",{})
try:
    try:
        if sys.argv[3]=="immediate":
            with (root/"log").open("x") as log:
                wait_run([sys.executable,"-c","pass"],log=log,env=os.environ.copy(),journal=j)
        else:
            cmd=[sys.executable,str(root/"server.py")] if sys.argv[3]=="normal" else [str(root/"absent-binary")]
            ev=Evaluator(cmd,process_journal=j)
            ev.close()
    except FileNotFoundError:
        assert sys.argv[3]=="failed"
finally:
    j.close()
''')
                result = subprocess.run([sys.executable, "-I", str(worker), str(HERE), str(root), mode],
                                        capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                verdict = process_verification.verify(root / "session")
                self.assertTrue(verdict["processes_stopped"], verdict["issues"])
                events = read_events(root / "session/events.jsonl")["events"]
                kinds = [e["kind"] for e in events]
                self.assertIn("process_spawn_intent", kinds)
                self.assertIn("process_spawn_reaped" if mode != "failed" else "process_spawn_failed", kinds)

    def test_forcibly_killed_launcher_and_run_leave_detectable_evaluator_orphans(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            server = root / "server.py"
            server.write_text('''import json,os,pathlib,subprocess,sys,time
root=pathlib.Path(sys.argv[1])
child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"])
(root/"owned.json").write_text(json.dumps([os.getpid(),child.pid]))
print(json.dumps({"op":"ready","ok":True}),flush=True)
for line in sys.stdin:
    (root/"waiting").touch()
    time.sleep(60)
''')
            worker = root / "worker.py"
            worker.write_text('''import pathlib,sys
sys.path.insert(0,sys.argv[1])
from evaluator import Evaluator
from session_journal import Journal
root=pathlib.Path(sys.argv[2]); j=Journal(root/"session/job.processes",{})
ev=Evaluator([sys.executable,str(root/"server.py"),str(root)],process_journal=j)
ev.request("wait")
''')
            launcher = root / "launcher.py"
            launcher.write_text('''import os,pathlib,sys
sys.path.insert(0,sys.argv[1])
from owned_child import wait_run
from session_journal import Journal
root=pathlib.Path(sys.argv[2]); j=Journal(root/"session",{})
with (root/"child.log").open("x") as log:
    wait_run([sys.executable,"-I",str(root/"worker.py"),sys.argv[1],str(root)],
             log=log,env=os.environ.copy(),journal=j,child_journal="job.processes")
''')
            proc = subprocess.Popen([sys.executable, "-I", str(launcher), str(HERE), str(root)],
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            run_pid = None
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if proc.poll() is not None:
                        self.fail(proc.communicate()[1].decode())
                    journal_path = root / "session/events.jsonl"
                    if journal_path.exists():
                        events = read_events(journal_path)["events"]
                        started = [e for e in events if e["kind"] == "process_spawn_started"]
                        if started and (root / "waiting").exists():
                            run_pid = started[0]["identity"]["pid"]
                            break
                    time.sleep(.01)
                self.assertIsNotNone(run_pid)
                proc.kill(); proc.communicate(timeout=5)
                verdict = process_verification.verify(root / "session")
                self.assertFalse(verdict["processes_stopped"])
                os.killpg(run_pid, signal.SIGKILL)
                time.sleep(.05)
                verdict = process_verification.verify(root / "session")
                self.assertFalse(verdict["processes_stopped"])
                self.assertTrue(any("job.processes/spawn" in s and "live" in s for s in verdict["issues"]), verdict)
                evaluator_pid = json.loads((root / "owned.json").read_text())[0]
                os.killpg(evaluator_pid, signal.SIGKILL)
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    verdict = process_verification.verify(root / "session")
                    if verdict["processes_stopped"]: break
                    time.sleep(.02)
                self.assertTrue(verdict["processes_stopped"], verdict["issues"])
                self.assertFalse(verdict["can_launch"])
                self.assertEqual(set(verdict["journals"]), {".", "job.processes"})
            finally:
                if proc.poll() is None: proc.kill(); proc.communicate()
                groups = [run_pid]
                if (root / "owned.json").exists(): groups.append(json.loads((root / "owned.json").read_text())[0])
                for pgid in groups:
                    if pgid is not None:
                        try: os.killpg(pgid, signal.SIGKILL)
                        except ProcessLookupError: pass


if __name__ == "__main__":
    unittest.main(verbosity=2)
