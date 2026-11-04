"""Private-copy mutations of stopped-run preservation and signal handling."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("completed_optimizer_lost", "run.py", 'trace["optimizer_completed"] = optimizer_completed',
     'trace["optimizer_completed"] = False',
     "InterruptionTests.test_stopped_snapshot_exists_before_diagnostic_and_cannot_be_overwritten"),
    ("diagnostic_on_interrupted_optimizer", "run.py",
     'and run is not None and optimizer_completed)', 'and run is not None)',
     "InterruptionTests.test_interrupted_optimizer_preserves_prefix_and_does_not_run_diagnostic"),
    ("pending_signal_ignored", "interruption.py", 'if self.requested is not None:', 'if False:',
     "InterruptionTests.test_signal_during_evidence_phase_is_deferred_and_handler_is_restored"),
    ("changed_diagnostic_selection_accepted", "run.py",
     'if physical_diagnostic.select(run) != selected:', 'if False:',
     "InterruptionTests.test_snapshot_reconstructs_diagnostic_without_optimizer_or_ledger"),
    ("changed_optimizer_result_accepted", "recovery_inventory.py",
     'if raw.get(key) != stopped.get(key):', 'if False:',
     "RecoveryInventoryTests.test_completed_optimizer_with_interrupted_diagnostic_is_never_a_rerun_candidate"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    cases = (("baseline", None, None, None, None), *MUTATIONS)
    for name, file, before, after, test in cases:
        candidate = dict(sources)
        if file:
            assert candidate[file].count(before) == 1, name
            candidate[file] = candidate[file].replace(before, after)
            compile(candidate[file], file, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-run-interruption-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            testfile = ("test_recovery_inventory.py" if file == "recovery_inventory.py"
                        else "test_run_interruption.py")
            cmd = [sys.executable, "-I", str(Path(td) / testfile)] + ([test] if test else [])
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=45)
        expected = (result.returncode == 0 if file is None else
                    result.returncode != 0 and "AssertionError" in result.stderr)
        rows.append({"name": name, "expected_result": expected, "exit_code": result.returncode,
                     "mutated_file": file, "test": test, "stderr": result.stderr})
    print(json.dumps({"python": sys.version,
        "baseline_source_sha256": {f: hashlib.sha256(s.encode()).hexdigest() for f, s in sources.items()},
        "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
