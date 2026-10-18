"""Private-copy regressions of policy selection, allowances and durable ownership."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("later_completion_selected", "recovery_policy.py", 'selected = merged["completed"][0]',
     'selected = merged["completed"][-1]', "test_recovery_policy.py",
     "PolicyTests.test_first_clean_scientific_failure_survives_later_better_results"),
    ("external_allowance_reset", "recovery_policy.py", 'elif latest["key"] in external_events and external_used == 0:',
     'elif latest["key"] in external_events:', "test_recovery_policy.py",
     "PolicyTests.test_external_allowance_does_not_reset_across_generations"),
    ("contamination_allowance_reset", "recovery_policy.py", 'and contamination_used == 0:',
     ':', "test_recovery_policy.py", "PolicyTests.test_contamination_allowance_carries_original_and_recovery_reruns"),
    ("recovery_before_remaining_queue", "recovery_policy.py", '(tail if attempts else primary).append(action)',
     '(primary if attempts else tail).append(action)', "test_recovery_policy.py",
     "PolicyTests.test_remaining_queue_precedes_original_order_recovery_tail"),
    ("external_evidence_not_verified", "recovery_policy.py", 'external_attestation(event, key)',
     'pass', "test_recovery_policy.py",
     "PolicyTests.test_signal_alone_unknown_crash_and_changed_attestation_never_authorize_replacement"),
    ("unfinalized_completed_optimizer_repeated", "recovery_policy.py",
     'elif any(a.get("unfinalized_trace", {}).get("optimizer_completed") is True for a in attempts):',
     'elif False:', "test_recovery_policy.py",
     "PolicyTests.test_unfinalized_completed_optimizer_never_becomes_whole_run_replacement"),
    ("later_diagnostic_selected", "recovery_policy.py", 'next((r for r in repairs if r["status"]',
     'next((r for r in reversed(repairs) if r["status"]', "test_recovery_policy.py",
     "PolicyTests.test_diagnostic_repair_preserves_optimizer_and_first_terminal_repair"),
    ("registry_lock_ignored", "recovery_registry.py", 'fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)',
     'pass', "test_recovery_registry.py",
     "RegistryTests.test_real_process_lock_excludes_second_launcher_and_releases_after_kill"),
    ("reserved_plan_hash_ignored", "recovery_registry.py",
     'if plan["file"] != f"plan-{event[\'index\']:04d}.json" or digest(self.root / plan["file"]) != plan["sha256"]:',
     'if False:', "test_recovery_registry.py",
     "RegistryTests.test_changed_origin_prior_journal_plan_or_link_refuses_reopen"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    cases = [("baseline_" + name, None, None, None, "test_recovery_" + name + ".py", None)
             for name in ("policy", "registry", "inventory")]
    rows = []
    for name, target, before, after, testfile, test in (*cases, *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate[target].count(before) == 1, name
            candidate[target] = candidate[target].replace(before, after)
            compile(candidate[target], target, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-recovery-policy-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / testfile)]
            if test: cmd.append(test)
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=45)
        expected = (result.returncode == 0 if before is None else
                    result.returncode != 0 and "AssertionError" in result.stderr)
        rows.append({"name": name, "expected_result": expected, "exit_code": result.returncode,
                     "test": test, "stderr": result.stderr})
    print(json.dumps({"python": sys.version,
        "baseline_source_sha256": {f: hashlib.sha256(s.encode()).hexdigest() for f, s in sources.items()},
        "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
