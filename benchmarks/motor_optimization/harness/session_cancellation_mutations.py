"""Private-copy regressions for launcher cancellation and lifetime receipts."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("cancelled_queue_dispatched", "session.py", 'while stop() is None:', 'while True:',
     "test_cancelled_host_gate_does_not_wait_or_dispatch"),
    ("cancelled_evaluator_spawned", "evaluator.py", 'interruption.check(stop)', 'interruption.check(None)',
     "test_cancelled_evaluator_never_spawns"),
    ("forced_exit_claimed_cleanup", "owned_child.py", 'descendant_cleanup_verified=False',
     'descendant_cleanup_verified=True', "test_forced_exit_is_reaped_but_never_complete"),
    ("missing_phase_receipts_accepted", "owned_child.py", 'complete = (receipt.get("available")',
     'complete = True or (receipt.get("available")', "test_completeness_requires_terminal_phase_receipts"),
    ("normal_child_receipt_dropped", "owned_child.py", 'receipt.update(wall_seconds=',
     'receipt.update(available=False, wall_seconds=',
     "test_concurrent_lifetimes_have_separate_exclusive_wait4_receipts"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, file, before, after, test in (("baseline", None, None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if file:
            assert candidate[file].count(before) == 1, name
            candidate[file] = candidate[file].replace(before, after)
            compile(candidate[file], file, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-session-cancellation-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_session_cancellation.py")]
            if test:
                cmd.append("CancellationTests." + test)
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
