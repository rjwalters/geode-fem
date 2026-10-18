"""Private-copy model/common-unit mutation checks for amendment 8."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("D_uses_primary_model", "driver.py",
     '"letter": "D", "optimizer": "cobyqa", "resolution": "L0"',
     '"letter": "D", "optimizer": "cobyqa", "resolution": None'),
    ("E_uses_wrong_optimizer", "driver.py",
     '"letter": "E", "optimizer": "slsqp", "resolution": "L0"',
     '"letter": "E", "optimizer": "cobyqa", "resolution": "L0"'),
    ("L0_gets_different_unit", "session.py",
     'unit = measured_unit if common_unit is None else float(common_unit)',
     'unit = measured_unit'),
    ("L0_changes_validator", "run.py", '"--val-mesh", s["val_mesh"]',
     '"--val-mesh", m["train_mesh"]'),
    ("E_loses_restoration_record", "driver.py",
     'self.restarts if METHOD_SPECS[self.method]["optimizer"] == "slsqp" else None',
     'self.restarts if self.method == "slsqp" else None'),
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
        with tempfile.TemporaryDirectory(prefix="motor-baseline-wiring-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            result = subprocess.run([sys.executable, "-I", str(Path(td) / "test_baseline_wiring.py")],
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
