"""Policy order, outcome preservation and replacement limits across generations."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from recovery_policy import attempt_key, plan
from session_journal import digest


def inventory(source="original", count=6):
    jobs = [{"name": f"job{k}", "position": k, "method": "slsqp", "space": "a",
             "start": "theta0", "repeat": 1} for k in range(count)]
    return {"source": source, "ready": True, "owner_active": False, "incomplete_tail_bytes": 0,
            "jobs": [{"job": j, "attempts": [], "eligible_completion": None} for j in jobs]}


def attempt(inv, index, *, reason=None, complete=False, clean=False, suffix="", deferred=False, diagnostic=False):
    row = inv["jobs"][index]
    record = {"attempt_id": 10 * index + len(row["attempts"]), "job": row["job"],
              "suffix": suffix, "recovery_reason": reason}
    if not deferred:
        record["dispatched"] = {"time": 100.}
    if complete:
        record["outcome"] = {"optimizer_completed": True, "stop_reason": "error",
                             "ledger": {"total_seconds": 30., "total_fe": 10.}}
        record["finished"] = {"result": {"host_contaminated": not clean, "exit": 1}}
        if clean:
            row["eligible_completion"] = {"attempt_id": record["attempt_id"], "trace": "trace.json", "trace_sha256": "fixture"}
            row["disposition"] = "completed_optimizer_diagnostic_pending" if diagnostic else "completed_preserve"
            if diagnostic:
                row["eligible_completion"]["optimizer_snapshot"] = {"file": "stopped.json", "sha256": "fixture"}
    row["attempts"].append(record)
    return attempt_key(inv["source"], record)


def attestation(root, key):
    path = root / "operator-event.txt"
    path.write_text("controlled fixture: externally stopped")
    return {"attempt": key, "verified_external_interruption": True, "event_kind": "operator_stop",
            "reason": "controlled test", "verified_by": "fixture reviewer", "event_id": "fixture-stop",
            "verification_reference": "fixture-reviewed-event", "evidence": [{"path": str(path), "sha256": digest(path)}]}


class PolicyTests(unittest.TestCase):
    def test_session_completion_preserves_scientific_failures_but_requires_finished_diagnostic(self):
        from session import completed_attempt, session_host_idle
        base = {"status": "ran", "exit": 1, "optimizer_completed": True,
                "stop_reason": "error", "diagnostic_status": "not_requested"}
        self.assertTrue(completed_attempt(base))
        for change in ({"optimizer_completed": False}, {"stop_reason": "interrupted"},
                       {"diagnostic_status": "pending"}, {"diagnostic_status": "interrupted"}):
            self.assertFalse(completed_attempt(dict(base, **change)))
        self.assertFalse(session_host_idle([{"job_name": "job", "host_contaminated": None}]))

    def test_remaining_queue_precedes_original_order_recovery_tail(self):
        with tempfile.TemporaryDirectory() as td:
            inv = inventory()
            attempt(inv, 0, deferred=True)
            key = attempt(inv, 1)
            attempt(inv, 2, complete=True, clean=True)
            attempt(inv, 4, complete=True)
            out = plan([inv], external_events={key: attestation(Path(td), key)})
            self.assertEqual([a["job"]["position"] for a in out["remaining_registered"]], [3, 5])
            self.assertEqual([a["job"]["position"] for a in out["recovery_tail"]], [0, 1, 4])
            self.assertEqual([a["reason"] for a in out["recovery_tail"]],
                             ["never_started_or_deferred", "external_interruption", "contamination"])
            self.assertFalse(out["can_launch"])

    def test_first_clean_scientific_failure_survives_later_better_results(self):
        first, later = inventory(), inventory("recovery1")
        key = attempt(first, 0, complete=True, clean=True)
        attempt(later, 0, reason="external_interruption", complete=True, clean=True)
        out = plan([first, later])
        self.assertEqual(out["jobs"][0]["selected_completion"]["key"], key)
        self.assertEqual(len(out["jobs"][0]["attempts"]), 2)
        self.assertFalse(any(a["job"]["position"] == 0 for a in out["recovery_tail"]))
        self.assertEqual(out["jobs"][0]["attempts"][0]["outcome"]["ledger"]["total_seconds"], 30.)

    def test_external_allowance_does_not_reset_across_generations(self):
        with tempfile.TemporaryDirectory() as td:
            first, later = inventory(), inventory("recovery1")
            attempt(first, 0)
            key = attempt(later, 0, reason="external_interruption")
            out = plan([first, later], external_events={key: attestation(Path(td), key)})
            self.assertEqual(out["jobs"][0]["disposition"], "unresolved")
            self.assertEqual(out["jobs"][0]["external_replacements_used"], 1)
            self.assertEqual(out["replacement_job_count"], 1)
            again = inventory("recovery2")
            attempt(again, 0, reason="external_interruption")
            with self.assertRaisesRegex(ValueError, "allowance"):
                plan([first, later, again])

    def test_contamination_allowance_carries_original_and_recovery_reruns(self):
        for original_rerun in (False, True):
            with self.subTest(original_rerun=original_rerun):
                first, later = inventory(), inventory("recovery1")
                attempt(first, 0, complete=True)
                if original_rerun:
                    attempt(first, 0, complete=True, suffix="_rerun1")
                else:
                    attempt(later, 0, reason="contamination", complete=True)
                out = plan([first, later])
                self.assertEqual(out["jobs"][0]["contamination_reruns_used"], 1)
                self.assertEqual(out["jobs"][0]["disposition"], "unresolved")
                self.assertFalse(any(a["job"]["position"] == 0 for a in out["recovery_tail"]))

    def test_deferred_replacement_does_not_spend_allowance_before_dispatch(self):
        with tempfile.TemporaryDirectory() as td:
            first, later = inventory(), inventory("recovery1")
            key = attempt(first, 0)
            attempt(later, 0, reason="external_interruption", deferred=True)
            out = plan([first, later], external_events={key: attestation(Path(td), key)})
            self.assertEqual(out["jobs"][0]["external_replacements_used"], 0)
            self.assertEqual(out["jobs"][0]["disposition"], "external_replacement")

    def test_signal_alone_unknown_crash_and_changed_attestation_never_authorize_replacement(self):
        with tempfile.TemporaryDirectory() as td:
            inv = inventory(); key = attempt(inv, 0)
            inv["jobs"][0]["attempts"][0]["outcome"] = {"optimizer_completed": False, "interruption": {"signal": 15}}
            self.assertEqual(plan([inv])["jobs"][0]["disposition"], "unresolved")
            with self.assertRaises(ValueError):
                plan([inv], external_events={key: {"signal": 15, "verified_external_interruption": True}})
            evidence = attestation(Path(td), key)
            Path(evidence["evidence"][0]["path"]).write_text("changed")
            with self.assertRaisesRegex(ValueError, "evidence changed"):
                plan([inv], external_events={key: evidence})

    def test_diagnostic_repair_preserves_optimizer_and_first_terminal_repair(self):
        inv = inventory(); key = attempt(inv, 0, complete=True, clean=True, diagnostic=True)
        out = plan([inv])
        self.assertEqual(out["jobs"][0]["disposition"], "diagnostic_only")
        self.assertEqual(len(out["diagnostics"]), 1)
        self.assertFalse(any(a["job"]["position"] == 0 for a in out["recovery_tail"]))
        repairs = [{"optimizer_attempt": key, "status": status, "attempt": k}
                   for k, status in enumerate(("interrupted", "failed", "ok"))]
        out = plan([inv], diagnostic_attempts=repairs)
        self.assertEqual(out["diagnostics"], [])
        self.assertEqual(out["jobs"][0]["selected_diagnostic"]["status"], "failed")
        self.assertEqual(out["jobs"][0]["diagnostic_attempts"], repairs)

    def test_unfinalized_completed_optimizer_never_becomes_whole_run_replacement(self):
        with tempfile.TemporaryDirectory() as td:
            inv = inventory(); key = attempt(inv, 0)
            inv["jobs"][0]["attempts"][0]["unfinalized_trace"] = {"optimizer_completed": True}
            out = plan([inv], external_events={key: attestation(Path(td), key)})
            self.assertEqual(out["jobs"][0]["disposition"], "unresolved")
            self.assertFalse(any(a["job"]["position"] == 0 for a in out["recovery_tail"]))

    def test_active_torn_reordered_or_unreasoned_chain_is_refused(self):
        first = inventory()
        for change in (lambda d: d.update(owner_active=True), lambda d: d.update(incomplete_tail_bytes=4),
                       lambda d: d["jobs"].reverse()):
            bad = copy.deepcopy(first); change(bad)
            with self.assertRaises(ValueError): plan([bad])
        later = inventory("recovery1"); attempt(later, 0)
        with self.assertRaisesRegex(ValueError, "policy reason"):
            plan([first, later])
        with self.assertRaisesRegex(ValueError, "repeated"):
            plan([first, first])


if __name__ == "__main__":
    unittest.main(verbosity=2)
