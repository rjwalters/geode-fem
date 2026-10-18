"""Private-copy regressions for the diagnostic-only recovery worker."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("snapshot_hash_ignored", 'if hashlib.sha256(raw).hexdigest() != expected_sha256:', 'if False:',
     "test_changed_snapshot_session_unit_model_anchor_or_completion_refuses_before_launch"),
    ("unit_binding_ignored", '("unit_seconds", unit)', '("unit_seconds", snapshot.get("unit_seconds"))',
     "test_changed_snapshot_session_unit_model_anchor_or_completion_refuses_before_launch"),
    ("completion_ignored", 'snapshot.get("optimizer_completed") is not True', 'False',
     "test_changed_snapshot_session_unit_model_anchor_or_completion_refuses_before_launch"),
    ("pending_stop_ignored", 'with interruption.phase(signals):', 'with interruption.phase(None):',
     "test_pending_stop_prevents_evaluator_launch"),
    ("optimizer_reentered", 'result = fn(stopped_run, make,', 'runner.execute(stopped_run, None)\n                result = fn(stopped_run, make,',
     "test_repair_matches_original_diagnostic_without_optimizer_or_ledger"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, before, after, test in (("baseline", None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate["diagnostic_repair.py"].count(before) == 1, name
            candidate["diagnostic_repair.py"] = candidate["diagnostic_repair.py"].replace(before, after)
            # Also remove the reconstruction helper's second completion guard,
            # to prove the test rejects incomplete optimizer evidence itself.
            if name == "completion_ignored":
                candidate["run.py"] = candidate["run.py"].replace(
                    'if snapshot.get("optimizer_completed") is not True or not state:', 'if not state:')
            compile(candidate["diagnostic_repair.py"], "diagnostic_repair.py", "exec")
        with tempfile.TemporaryDirectory(prefix="motor-diagnostic-repair-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_diagnostic_repair.py")]
            if test:
                cmd.append("RepairTests." + test)
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
