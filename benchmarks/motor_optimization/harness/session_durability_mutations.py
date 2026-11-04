"""Private-copy session durability and recovery-inventory mutations."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("different_recovery_unit_accepted", "recovery_inventory.py",
     'or calibration["calibration"].get("unit_seconds") != unit', 'or False'),
    ("contaminated_calibration_accepted", "session.py",
     'and cal["host"].get("host_contaminated") is False', 'and True'),
    ("immutable_evidence_overwritten", "session_journal.py",
     'with path.open("x") as stream:', 'with path.open("w") as stream:'),
    ("finished_attempt_not_durable", "session.py",
     'journal.emit("attempt_finished", attempt_id=attempt, result=result)',
     'journal.emit("attempt_observed", attempt_id=attempt, result=result)'),
    ("last_completion_selected", "recovery_inventory.py", 'selected, result = completed[0]',
     'selected, result = completed[-1]'),
    ("changed_trace_accepted", "recovery_inventory.py",
     'if digest(trace) != result["trace_sha256"]:', 'if False:'),
    ("interrupted_attempt_may_restart", "recovery_inventory.py",
     '"interrupted_unresolved", False, None', '"interrupted_unresolved", True, None'),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, file, before, after in (("baseline", None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if file:
            assert candidate[file].count(before) == 1, name
            candidate[file] = candidate[file].replace(before, after)
            compile(candidate[file], file, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-session-durability-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            result = subprocess.run([sys.executable, "-I", str(Path(td) / ("test_recovery_inventory.py" if file == "recovery_inventory.py" else "test_session_journal.py"))],
                                    capture_output=True, text=True, timeout=30)
        passed = result.returncode == 0 if file is None else (
            result.returncode != 0 and "AssertionError" in result.stderr)
        rows.append({"name": name, "expected_result": passed, "exit_code": result.returncode,
                     "mutated_file": file, "stderr": result.stderr})
    print(json.dumps({"python": sys.version,
                      "baseline_source_sha256": {f: hashlib.sha256(s.encode()).hexdigest()
                                                 for f, s in sources.items()},
                      "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
