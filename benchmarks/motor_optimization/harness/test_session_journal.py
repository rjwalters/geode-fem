"""Durable session and pre-dispatch failure checks; no FEM."""
import fcntl
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import session
from session_journal import Journal, freeze, read_events


def good_cal():
    return {"host": {"gate": True, "host_contaminated": False}, "deterministic": True,
            "anchor_agrees_4ulp": True, "space_b_theta0_train_agrees_bitwise": True,
            "unit_seconds": 3., "theta0_train": {"F": 1., "mean_full": 2.}, "weights": {}}


class JournalTests(unittest.TestCase):
    def test_parallel_appends_survive_close_and_refuse_reuse(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"
            j = Journal(root, {"fixture": True})
            def work(k):
                for n in range(5): j.emit("fixture", worker=k, index=n)
            workers = [threading.Thread(target=work, args=(k,)) for k in range(4)]
            for t in workers: t.start()
            for t in workers: t.join()
            j.close()
            data = read_events(root / "events.jsonl")
            self.assertEqual(len(data["events"]), 21)
            self.assertEqual(data["incomplete_tail_bytes"], 0)
            original = (root / "events.jsonl").read_bytes()
            with self.assertRaises(FileExistsError): Journal(root, {})
            self.assertEqual((root / "events.jsonl").read_bytes(), original)
            with (root / "owner.lock").open() as f:
                fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_torn_final_event_never_becomes_a_completed_attempt(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; j = Journal(root, {})
            j.emit("attempt_created", job={"name": "x"}); j.close()
            p = root / "events.jsonl"
            with p.open("ab") as f: f.write(b'{"sequence":2,"kind":"attempt_finished"')
            result = read_events(p)
            self.assertEqual(len(result["events"]), 2)
            self.assertGreater(result["incomplete_tail_bytes"], 0)
            self.assertFalse(any(e["kind"] == "attempt_finished" for e in result["events"]))
            with p.open("ab") as f: f.write(b'\n')
            with self.assertRaises(json.JSONDecodeError): read_events(p)

    def test_immutable_calibration_is_archived_before_rejection(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; j = Journal(root, {})
            cal = good_cal(); cal["host"]["host_contaminated"] = True
            with self.assertRaisesRegex(SystemExit, "calibration rejected"):
                session.accept_calibration(cal, root, "calibration.json", True, j)
            j.close()
            saved = json.loads((root / "calibration.json").read_text())
            self.assertFalse(saved["accepted_for_dispatch"])
            self.assertEqual(saved["calibration"], cal)
            with self.assertRaises(FileExistsError): freeze(root / "calibration.json", {})
            self.assertEqual(read_events(root / "events.jsonl")["events"][-1]["kind"], "calibration_completed")

    def test_rejected_primary_or_secondary_calibration_cannot_dispatch(self):
        for bad_index in (0, 1):
            with self.subTest(bad_index=bad_index), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"; binary = Path(td) / "eval"; binary.write_bytes(b"fixture")
                cals = [good_cal(), good_cal()]; cals[bad_index]["host"]["host_contaminated"] = True
                with patch.object(session, "host_record", return_value={}), \
                     patch.object(session, "provenance", return_value={}), \
                     patch.object(session, "sha256", return_value="fixture-hash"), \
                     patch("start_preflight.verify", return_value={"status": "PASS"}), \
                     patch.object(session, "gated_calibration", side_effect=cals), \
                     patch.object(session, "schedule") as dispatch:
                    with self.assertRaisesRegex(SystemExit, "calibration rejected"):
                        session.main(["--main", "--procs", "1", "--out", str(root), "--binary", str(binary)])
                    dispatch.assert_not_called()
                events = read_events(root / "events.jsonl")["events"]
                self.assertEqual(events[-1]["kind"], "session_failed")
                self.assertFalse(any(e["kind"] == "session_ready" for e in events))
                with (root / "owner.lock").open() as f:
                    fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_main_freezes_full_queue_and_transient_deferral_is_not_an_unresolved_job(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); out = root / "session"; meshes = root / "meshes"; meshes.mkdir()
            binary = root / "eval"; binary.write_bytes(b"fixture")
            for level in (0, 1, 2):
                (meshes / f"icem_initial_sector_L{level}.msh").write_bytes(f"fixture{level}".encode())
            (root / "anchor.json").write_text("{}")
            (root / "starts.json").write_text("{}")
            def preflight(binary, starts, meshes, path, **kwargs):
                record = {"status": "PASS"}; freeze(path, record); return record
            def schedule(jobs, *a, **k):
                rows = [dict(job, job_name=job["name"], status="ran", exit=0, host_contaminated=False)
                        for job in jobs]
                return [dict(rows[0], status="deferred", exit=None, host_contaminated=None), *rows]
            with patch.object(session, "host_record", return_value={}), \
                 patch.object(session, "provenance", return_value={}), \
                 patch.object(session, "MESHES", meshes), patch.object(session, "OPT", root), \
                 patch.object(session, "HERE", root), \
                 patch("recovery_bindings.capture", return_value={"schema": 1, "source": {"tree_clean": True}}) as capture, \
                 patch("recovery_bindings.validate_setup"), \
                 patch("start_preflight.verify", side_effect=preflight), \
                 patch.object(session, "gated_calibration", side_effect=[good_cal() for _ in range(4)]), \
                 patch.object(session, "schedule", side_effect=schedule) as dispatch, contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(session.main(["--main", "--procs", "1", "--out", str(out),
                                               "--binary", str(binary)]), 0)
                count = dispatch.call_count
                capture.return_value = {"schema": 1, "source": {"tree_clean": False}}
                with self.assertRaisesRegex(SystemExit, "clean, frozen"):
                    session.main(["--main", "--procs", "1", "--out", str(root / "dirty"),
                                  "--binary", str(binary)])
                self.assertEqual(dispatch.call_count, count)
                self.assertFalse(any(e["kind"] == "session_ready" for e in
                                     read_events(root / "dirty/events.jsonl")["events"]))
            manifest = json.loads((out / "manifest.json").read_text())
            self.assertTrue(manifest["complete"])
            self.assertEqual(manifest["unresolved_jobs"], [])
            jobs = json.loads((out / "queue.json").read_text())["jobs"]
            self.assertEqual(len(jobs), 100)
            bindings = json.loads((out / "bindings.json").read_text())
            from session_journal import digest
            for name, sha in bindings["files"].items():
                self.assertEqual(digest(out / name), sha)
            self.assertIn("recovery-inputs.json", bindings["files"])
            self.assertEqual(set(bindings["meshes"]["training_models"]), {"L0", "L1"})

    def test_load_samples_are_saved_before_monitor_stop(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; j = Journal(root, {})
            monitor = session.LoadMonitor(interval=.01, load=lambda: (2., 0., 0.), journal=j).start()
            deadline = time.monotonic() + 2
            while not monitor.samples and time.monotonic() < deadline: time.sleep(.005)
            monitor.stop(); j.close()
            events = read_events(root / "events.jsonl")["events"]
            self.assertTrue(any(e["kind"] == "load_sample" and e["load_one_minute"] == 2. for e in events))
            self.assertIsNone(monitor.error)

    def test_worker_exception_is_not_lost_from_scheduler_results(self):
        jobs = [{"name": "failed", "position": 0}, {"name": "done", "position": 1}]
        def run(job, ctx, suffix):
            if job["name"] == "failed":
                raise OSError("fixture process launch failure")
            return dict(job, job_name=job["name"], status="ran", exit=0, host_contaminated=False)
        results = session.schedule(jobs, {"procs": 2}, 1, run=run)
        self.assertEqual(len(results), 2)
        self.assertEqual(results[0]["status"], "error")
        self.assertIn("process launch failure", results[0]["error"])

    def test_attempt_started_and_finished_are_durable_and_trace_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; j = Journal(root, {}); (root / "logs").mkdir()
            ctx = {"out": root, "journal": j, "monitor": SimpleNamespace(samples=[], error=None),
                   "procs": 1, "max_wait": 1, "interval": .01, "gate": True}
            job = {"name": "slsqp_a_theta0_r1", "method": "slsqp", "space": "a", "start": "theta0",
                   "repeat": 1, "position": 0}
            def spawn(cmd, **kw):
                p = Path(cmd[cmd.index("--out") + 1]); p.write_text(json.dumps({"stop_reason": "budget"}))
                return SimpleNamespace(returncode=0)
            result = session.run_job(job, ctx, wait=lambda *a, **k: (True, 0., 1.), spawn=spawn)
            raw = (root / result["trace"]).read_bytes()
            with self.assertRaises(FileExistsError):
                session.run_job(job, ctx, wait=lambda *a, **k: (True, 0., 1.), spawn=spawn)
            j.close()
            events = read_events(root / "events.jsonl")["events"]
            self.assertEqual([e["kind"] for e in events], ["session_created", "attempt_created",
                             "attempt_dispatched", "attempt_finished", "attempt_created", "attempt_failed"])
            self.assertEqual((root / result["trace"]).read_bytes(), raw)
            self.assertIsNotNone(result["trace_sha256"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
