"""Bounded semantic checks of the standalone #1202 preparatory modules."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
FILES = ("main_schedule.py", "resource_usage.py", "test_main_preparation.py")
MUTATIONS = (
    ("ignore_start_rotation", "main_schedule.py",
     "LETTERS[start_index:] + LETTERS[:start_index]", "LETTERS"),
    ("repeat2_not_reversed", "main_schedule.py", "else order[::-1]", "else order"),
    ("omit_fifth_start", "main_schedule.py", '"lhs3", "lhs4"', '"lhs3"'),
    ("only_one_timing_repeat", "main_schedule.py", "for repeat in (1, 2):", "for repeat in (1,):"),
    ("linux_rss_as_bytes", "resource_usage.py", 'raw * 1024, "KiB"', 'raw, "KiB"'),
    ("macos_rss_as_kib", "resource_usage.py", 'raw, "bytes"', 'raw * 1024, "bytes"'),
    ("double_child_cpu", "resource_usage.py",
     'sum(p["total_seconds"] for p in parts)', 'sum(p["total_seconds"] for p in parts) + parts[1]["total_seconds"]'),
    ("rss_as_phase_delta", "resource_usage.py", '"bytes": b.get("maxrss_bytes")',
     '"bytes": (b.get("maxrss_bytes", 0) - a.get("maxrss_bytes", 0))'),
    ("default_claims_complete", "resource_usage.py", "children_complete=False", "children_complete=True"),
    ("self_cpu_used_for_children", "resource_usage.py", "resource.RUSAGE_CHILDREN", "resource.RUSAGE_SELF"),
)


def main():
    sources = {name: (HERE / name).read_text() for name in FILES}
    rows = []
    for name, filename, before, after in (("baseline", None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if filename:
            if candidate[filename].count(before) != 1:
                raise ValueError(f"{name}: expected exactly one mutation anchor")
            candidate[filename] = candidate[filename].replace(before, after)
            compile(candidate[filename], filename, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-main-preparation-") as td:
            root = Path(td)
            for file, source in candidate.items():
                (root / file).write_text(source)
            r = subprocess.run([sys.executable, "-I", str(root / "test_main_preparation.py")],
                               capture_output=True, text=True, timeout=30)
        expected = r.returncode == 0 if filename is None else r.returncode != 0
        rows.append({"name": name, "expected_result": expected, "exit_code": r.returncode,
                     "source_sha256": {f: hashlib.sha256(s.encode()).hexdigest()
                                       for f, s in candidate.items()},
                     "stdout": r.stdout, "stderr": r.stderr})
    print(json.dumps({"python": sys.version, "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
