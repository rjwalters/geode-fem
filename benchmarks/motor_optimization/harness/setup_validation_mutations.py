"""Private-copy setup preflight and FD evidence mutation checks."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("wrong_design_bits_accepted", "start_preflight.py",
     'if reply.get("x_bits") != to_bits(x):', 'if False:'),
    ("lower_start_quality_floor", "start_preflight.py", 'angle < 5.0', 'angle < 4.0'),
    ("failed_preflight_still_launches", "session.py",
     'if preflight["status"] != "PASS":', 'if False:'),
    ("select_better_fd_step", "l0_gradient_check.py",
     'selected = next(row for row in rows if row["h"] == 1e-4)',
     'selected = min(rows, key=lambda row: row["rel_dF"] or 0.)'),
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
        with tempfile.TemporaryDirectory(prefix="motor-setup-validation-") as td:
            for filename, source in candidate.items():
                if filename in ("test_start_preflight.py", "test_l0_gradient_check.py"):
                    source = source.replace("import start_preflight as preflight", "import start_preflight as preflight\npreflight.HERE = Path(" + repr(str(HERE)) + ")")
                    source = source.replace("import l0_gradient_check as check", "import l0_gradient_check as check\ncheck.HERE = Path(" + repr(str(HERE)) + ")")
                (Path(td) / filename).write_text(source)
            result = subprocess.run([sys.executable, "-I", str(Path(td) / ("test_l0_gradient_check.py" if file == "l0_gradient_check.py" else "test_start_preflight.py"))],
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
