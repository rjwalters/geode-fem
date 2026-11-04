"""Private-copy regressions of explicit linear-algebra runtime bindings."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("fingerprint_not_captured", "runtime_fingerprint.py", 'result["linear_algebra"] = linear_algebra(packages)', 'pass',
     "test_capture_records_backend_and_abi_despite_unchanged_payload"),
    ("abi_config_discarded", "runtime_fingerprint.py", 'dependencies[name][kind] = dep', 'dependencies[name][kind] = {}',
     "test_capture_records_backend_and_abi_despite_unchanged_payload"),
    ("runtime_change_ignored", "recovery_bindings.py", 'changed = [key for key in observed if observed[key] != expected.get(key)]',
     'changed = [key for key in observed if key != "runtime" and observed[key] != expected.get(key)]',
     "test_backend_version_abi_and_os_changes_refuse_recovery"),
)



def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, target, before, after, test in (("baseline", None, None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate[target].count(before) == 1, name
            candidate[target] = candidate[target].replace(before, after)
            compile(candidate[target], target, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-blas-binding-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_blas_bindings.py")]
            if test: cmd.append("BlasTests." + test)
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
