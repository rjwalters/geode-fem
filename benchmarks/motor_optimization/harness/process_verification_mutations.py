"""Private-copy regressions of recovery's read-only process absence check."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("live_group_ignored", 'if members:', 'if False:',
     "test_live_group_without_its_original_leader_blocks_recovery"),
    ("pid_reuse_ignored", 'issues.append(f"{label}: PID identity differs; never act on a reused PID")', 'pass',
     "test_reused_pid_or_changed_boot_is_never_treated_as_safe_absence"),
    ("fork_gap_ignored", 'issues.append(f"{label}: spawn intent has no published process identity")', 'pass',
     "test_fork_gap_torn_tail_and_missing_child_journal_are_unresolved"),
    ("changed_boot_ignored", 'if not identity or identity.get("host") != current_host:', 'if not identity:',
     "test_reused_pid_or_changed_boot_is_never_treated_as_safe_absence"),
    ("owner_lock_ignored", 'if active:', 'if False:',
     "test_live_owner_lock_blocks_even_an_empty_process_observation"),
    ("evaluator_journal_not_checked", 'visit(directory / child)', 'pass',
     "test_forcibly_killed_launcher_and_run_leave_detectable_evaluator_orphans"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, before, after, test in (("baseline", None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate["process_verification.py"].count(before) == 1, name
            candidate["process_verification.py"] = candidate["process_verification.py"].replace(before, after)
            compile(candidate["process_verification.py"], "process_verification.py", "exec")
        with tempfile.TemporaryDirectory(prefix="motor-process-verification-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_process_verification.py")]
            if test: cmd.append("VerificationTests." + test)
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
