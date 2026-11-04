"""Detect lifecycle regressions in disposable copies, never the working tree."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("startup_cleanup_missing",
     "            try:\n                self.kill()\n            except BaseException as cleanup_error:",
     "            try:\n                pass  # deliberate missing cleanup\n            except BaseException as cleanup_error:"),
    ("forced_exit_receipt_dropped", "        self._reap(block=True)",
     "        self.proc.wait()"),
    ("killed_child_falsely_complete", '            receipt["cpu_complete"] &= receipt["returncode"] == 0',
     '            receipt["cpu_complete"] = True'),
)


def main():
    sources = {p.name: p.read_bytes() for p in HERE.glob("*.py")}
    records = []
    with tempfile.TemporaryDirectory(prefix="motor-resource-lifecycle-") as temporary:
        root = Path(temporary)
        for name, raw in sources.items():
            (root / name).write_bytes(raw)
        original = sources["evaluator.py"].decode()
        for name, old, new in (("baseline", None, None), *MUTATIONS):
            if old is not None:
                assert original.count(old) == 1, name
            (root / "evaluator.py").write_text(original if old is None else original.replace(old, new))
            # Avoid same-size, same-second bytecode from a preceding mutant.
            shutil.rmtree(root / "__pycache__", ignore_errors=True)
            result = subprocess.run(
                [sys.executable, "-B", "-I", str(root / "test_evaluator_resources.py")],
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=30)
            passed = result.returncode == 0 if old is None else (
                result.returncode != 0 and "AssertionError" in result.stdout and "FAIL:" in result.stdout)
            records.append(dict(name=name, exit=result.returncode, expected_outcome_observed=passed,
                                output=result.stdout))
    report = dict(python=sys.version, source_sha256={n: hashlib.sha256(b).hexdigest()
                  for n, b in sorted(sources.items())}, checks=records,
                  scope="Lifecycle tests and three deliberate regressions; no FEM or performance claim.")
    (HERE / "resource-lifecycle-checks.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({r["name"]: r["expected_outcome_observed"] for r in records}))
    return 0 if all(r["expected_outcome_observed"] for r in records) else 1


if __name__ == "__main__":
    sys.exit(main())
