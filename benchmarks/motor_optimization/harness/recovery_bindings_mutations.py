"""Private-copy regressions of frozen recovery bindings."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("inputs_not_compared", "recovery_bindings.py", 'if observed[key] != expected.get(key)',
     'if key != "inputs" and observed[key] != expected.get(key)', "test_each_external_input_change_is_detected"),
    ("runtime_not_compared", "recovery_bindings.py", 'if observed[key] != expected.get(key)',
     'if key != "runtime" and observed[key] != expected.get(key)', "test_changed_unit_workers_settings_source_or_runtime_refuses"),
    ("host_not_compared", "recovery_bindings.py", 'if observed[key] != expected.get(key)',
     'if key != "host" and observed[key] != expected.get(key)', "test_changed_unit_workers_settings_source_or_runtime_refuses"),
    ("dirty_original_allowed", "recovery_bindings.py", 'expected["source"].get("tree_clean") is not True',
     'False', "test_changed_unit_workers_settings_source_or_runtime_refuses"),
    ("source_content_not_hashed", "recovery_bindings.py", 'files = {name: runtime_fingerprint.sha(repo / name) for name in names if name}',
     'files = {name: "unchanged" for name in names if name}', "test_real_git_manifest_observes_dirty_content_new_sources_and_failures"),
    ("installed_content_not_hashed", "runtime_fingerprint.py", 'files[str(entry)] = sha(path)',
     'files[str(entry)] = "unchanged"', "test_installed_payload_is_hashed_not_only_package_version"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, target, before, after, test in (("baseline", None, None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if before:
            assert candidate[target].count(before) == 1, name
            candidate[target] = candidate[target].replace(before, after)
            # Equality would also catch a dirty original; remove that second
            # defense in this mutation to exercise the clean-source requirement.
            if name == "dirty_original_allowed":
                candidate[target] = candidate[target].replace(
                    'if observed[key] != expected.get(key)',
                    'if key != "source" and observed[key] != expected.get(key)')
            compile(candidate[target], target, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-recovery-bindings-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_recovery_bindings.py")]
            if test: cmd.append("BindingTests." + test)
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
