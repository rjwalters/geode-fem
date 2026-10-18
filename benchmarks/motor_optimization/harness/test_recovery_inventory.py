"""Inventory preserves completed/scientific failures and exposes unresolved attempts."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from recovery_inventory import inventory
from session_journal import Journal, digest, freeze


class RecoveryInventoryTests(unittest.TestCase):
    def prepare(self, root, calibration_unit=3., calibration_accepted=True):
        j = Journal(root, {"label": "main"})
        jobs = [{"name": name, "method": "slsqp", "space": "a", "start": "theta0", "repeat": 1,
                 "position": k} for k, name in enumerate(("done", "scientific", "contaminated", "interrupted", "new", "deferred"))]
        freeze(root / "queue.json", {"jobs": jobs})
        freeze(root / "session.json", {"unit_seconds": 3., "budget_fe": 120.})
        freeze(root / "calibration_primary.json", {"accepted_for_dispatch": calibration_accepted, "calibration": {"unit_seconds": calibration_unit}})
        freeze(root / "bindings.json", {"files": {n: digest(root / n) for n in
                ("queue.json", "session.json", "calibration_primary.json")}})
        j.emit("session_ready", bindings_sha256=digest(root / "bindings.json"))
        return j, jobs

    def complete(self, j, job, suffix="", contaminated=False, reason="budget", objective=10):
        attempt = j.emit("attempt_created", job=job, suffix=suffix)
        j.emit("attempt_dispatched", attempt_id=attempt, trace=job["name"] + suffix + ".json")
        name = job["name"] + suffix + ".json"
        freeze(j.root / name, dict(job, stop_reason=reason, objective=objective))
        j.emit("attempt_finished", attempt_id=attempt, result={"status": "ran", "exit": 0,
               "trace": name, "trace_sha256": digest(j.root / name), "host_contaminated": contaminated})
        return attempt

    def test_classifications_preserve_all_attempts_and_never_choose_best_outcome(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; j, jobs = self.prepare(root)
            first = self.complete(j, jobs[0], objective=10)
            self.complete(j, jobs[0], suffix="_later", objective=1)
            self.complete(j, jobs[1], reason="restoration_failed")
            self.complete(j, jobs[2], contaminated=True)
            a = j.emit("attempt_created", job=jobs[3], suffix="")
            j.emit("attempt_dispatched", attempt_id=a, command=["fixture"])
            a = j.emit("attempt_created", job=jobs[5], suffix="")
            j.emit("attempt_finished", attempt_id=a, result={"status": "deferred"})
            self.assertTrue(inventory(root)["owner_active"])
            j.close()
            raw = (root / "events.jsonl").read_bytes()
            result = inventory(root)
            self.assertFalse(result["owner_active"])
            self.assertFalse(result["can_launch"])
            rows = {r["job"]["name"]: r for r in result["jobs"]}
            self.assertEqual(rows["done"]["eligible_completion"]["attempt_id"], first)
            self.assertEqual(len(rows["done"]["attempts"]), 2)
            self.assertEqual(rows["scientific"]["disposition"], "completed_preserve")
            self.assertEqual(rows["contaminated"]["disposition"], "contaminated_unresolved")
            self.assertEqual(rows["interrupted"]["disposition"], "interrupted_unresolved")
            self.assertFalse(rows["interrupted"]["never_started_launch_candidate"])
            for name in ("new", "deferred"):
                self.assertTrue(rows[name]["never_started_launch_candidate"])
            self.assertEqual(raw, (root / "events.jsonl").read_bytes())

    def test_changed_calibration_or_trace_is_not_silently_accepted(self):
        for artifact in ("session.json", "done.json"):
            with self.subTest(artifact=artifact), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"; j, jobs = self.prepare(root)
                self.complete(j, jobs[0]); j.close()
                p = root / artifact; p.write_text(p.read_text() + " ")
                with self.assertRaisesRegex(ValueError, "changed"):
                    inventory(root)

    def test_rejected_calibration_or_different_unit_cannot_be_recovered(self):
        for unit, accepted in ((3., False), (4., True)):
            with self.subTest(unit=unit, accepted=accepted), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"
                j, _ = self.prepare(root, unit, accepted); j.close()
                with self.assertRaisesRegex(ValueError, "calibration or changed validation unit"):
                    inventory(root)

    def test_completed_optimizer_with_interrupted_diagnostic_is_never_a_rerun_candidate(self):
        for tamper in (False, True):
            with self.subTest(tamper=tamper), tempfile.TemporaryDirectory() as td:
                root = Path(td) / "session"; j, jobs = self.prepare(root)
                job = jobs[0]
                attempt = j.emit("attempt_created", job=job, suffix="")
                j.emit("attempt_dispatched", attempt_id=attempt, trace="done.json")
                stopped = dict(job, unit_seconds=3., budget_fe=120., optimizer_completed=True,
                               stop_reason="budget", diagnostic_status="pending",
                               diagnostic_state={"kind": "fixture"}, ledger={"total_fe": 120.},
                               attainment={"T50": {"attained": True}})
                binding = freeze(root / "done.optimizer-stopped.json", stopped)
                raw = dict(stopped, optimizer_snapshot=binding, diagnostic_status="interrupted")
                if tamper:
                    raw["ledger"] = {"total_fe": 60.}
                freeze(root / "done.json", raw)
                j.emit("attempt_finished", attempt_id=attempt, result={"status": "ran", "exit": 143,
                       "trace": "done.json", "trace_sha256": digest(root / "done.json"),
                       "host_contaminated": False})
                j.close()
                if tamper:
                    with self.assertRaisesRegex(ValueError, "stopped optimizer result changed"):
                        inventory(root)
                else:
                    row = inventory(root)["jobs"][0]
                    self.assertEqual(row["disposition"], "completed_optimizer_diagnostic_pending")
                    self.assertFalse(row["never_started_launch_candidate"])
                    self.assertEqual(row["eligible_completion"]["optimizer_snapshot"], binding)

    def test_torn_finished_event_leaves_interrupted_attempt_unresolved(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "session"; j, jobs = self.prepare(root)
            a = j.emit("attempt_created", job=jobs[0], suffix="")
            j.emit("attempt_dispatched", attempt_id=a); j.close()
            with (root / "events.jsonl").open("ab") as f:
                f.write(b'{"kind":"attempt_finished"')
            result = inventory(root)
            self.assertGreater(result["incomplete_tail_bytes"], 0)
            self.assertEqual(result["jobs"][0]["disposition"], "interrupted_unresolved")


if __name__ == "__main__":
    unittest.main(verbosity=2)
