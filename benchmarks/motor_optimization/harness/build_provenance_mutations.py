"""Private-copy regressions of witnessed evaluator build bindings."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("source_binding_ignored", "build_provenance.py", 'or value.get("source") != source', 'or False',
     "test_changed_source_or_any_bound_evidence_is_refused"),
    ("binary_binding_ignored", "build_provenance.py", 'if digest(binary) != value["binary"]["sha256"]:', 'if False:',
     "test_real_child_receipt_pins_snapshot_without_requiring_mutable_target"),
    ("changing_source_accepted", "build_provenance.py", 'if source_probe(repo) != source or environment_probe(repo, env) != build_env:',
     'if False:', "test_dirty_or_changing_sources_never_publish_success"),
    ("main_build_gate_skipped", "session.py", 'if a.main or a.build_receipt is not None:', 'if False:',
     "test_main_without_receipt_stops_before_calibration_or_preflight"),
)



def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, target, before, after, test in (("baseline", None, None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate[target].count(before) == 1, name
            candidate[target] = candidate[target].replace(before, after)
            if name == "source_binding_ignored":
                candidate[target] = candidate[target].replace('if inputs.get("source") != source:', 'if False:')
            compile(candidate[target], target, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-build-provenance-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_build_provenance.py")]
            if test: cmd.append("BuildTests." + test)
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
