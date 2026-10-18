"""Private-copy physical-diagnostic mutation checks for amendment 8."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("highest_F_selected", "physical_diagnostic.py",
     'key=lambda v: (v["F_val"], v["index"])',
     'key=lambda v: (-v["F_val"], v["index"])'),
    ("late_validation_eligible", "physical_diagnostic.py",
     'if v.get("credited") and v.get("status") == "ok"',
     'if v.get("status") == "ok"'),
    ("last_trial_is_incumbent", "driver.py",
     'if self.training_incumbent is None:', 'if True:'),
    ("waveform_statistics_not_checked", "physical_diagnostic.py",
     'raise ValueError(f"{name}: waveform does not reproduce mean/std")', 'pass'),
    ("training_pair_omitted", "driver.py",
     'paired_training=paired_training, attained=flags, credited=credited,',
     'paired_training=None, attained=flags, credited=credited,'),
    ("fallback_claims_feasible", "physical_diagnostic.py",
     '"validation": None, "common_feasibility_established": False',
     '"validation": None, "common_feasibility_established": True'),
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
        with tempfile.TemporaryDirectory(prefix="motor-physical-diagnostic-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            result = subprocess.run([sys.executable, "-I", str(Path(td) / "test_physical_diagnostic.py")],
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
