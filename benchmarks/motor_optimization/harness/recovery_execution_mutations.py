"""Private-copy end-to-end regressions of linked recovery dispatch."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("dispatch_check_skipped", "session.py", 'if ctx.get("before_dispatch") is not None:',
     'if False:', "test_fresh_dispatch_verification_failure_launches_nothing"),
    ("live_old_process_ignored", "recovery.py", 'if process.get("processes_stopped") is not True:',
     'if False:', "test_real_orphan_blocks_reservation_and_launch"),
    ("negative_binding_verdict_ignored", "recovery.py", 'if binding_verifier(frozen, config).get("verified") is not True:',
     'if False:', "test_negative_binding_verdict_cannot_be_treated_as_permission"),
    ("recovery_queue_first", "recovery.py", 'for phase in ("remaining_registered", "recovery_tail"):',
     'for phase in ("recovery_tail", "remaining_registered"):',
     "test_complete_linked_recovery_preserves_source_order_unit_and_diagnostic"),
    ("old_generations_omitted", "recovery_history.py", 'paths = [registry.source, *[Path(g["path"]) for g in generations]]',
     'paths = [registry.source]', "test_complete_linked_recovery_preserves_source_order_unit_and_diagnostic"),
    ("repaired_input_binding_ignored", "recovery_history.py",
     'or raw.get("inputs") != {"file": "inputs.json", "sha256": digest(directory / "inputs.json")}',
     'or False', "test_changed_diagnostic_inputs_block_later_recovery"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, target, before, after, test in (("baseline", None, None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate[target].count(before) == 1, name
            candidate[target] = candidate[target].replace(before, after)
            if name == "repaired_input_binding_ignored":
                # Remove both checks of the changed session payload; the test
                # should fail if repair inputs are accepted without their binding.
                candidate[target] = candidate[target].replace('or inputs.get("session") != expected_session', 'or False')
            compile(candidate[target], target, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-recovery-execution-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_recovery_execution.py")]
            if test: cmd.append("ExecutionTests." + test)
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=90)
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
