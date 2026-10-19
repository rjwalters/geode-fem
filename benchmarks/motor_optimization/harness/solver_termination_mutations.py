"""Private-copy regressions of pinned solver cap reporting."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
MUTATIONS = (
    ("slsqp_cap_lost", "driver.py", '{9: "maxiter"}', '{8: "maxiter"}',
     "test_actual_pinned_slsqp_iteration_cap"),
    ("cobyqa_eval_cap_lost", "driver.py", '{5: "maxfev", 6: "maxiter"}', '{4: "maxfev", 6: "maxiter"}',
     "test_actual_pinned_cobyqa_caps_are_distinct"),
    ("cap_called_convergence", "driver.py", 'return "optimizer_cap", msg, None', 'return "converged", msg, None',
     "test_top_level_all_four_methods_keep_caps_and_failed_exits"),
    ("subproblem_called_optimizer", "driver.py", 'recorded_minimize(run, "surrogate_subproblem", models.f',
     'recorded_minimize(run, "optimizer", models.f', "test_surrogate_local_cap_is_not_a_whole_run_cap"),
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
        with tempfile.TemporaryDirectory(prefix="motor-solver-termination-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            cmd = [sys.executable, "-I", str(Path(td) / "test_solver_termination.py")]
            if test: cmd.append("TerminationTests." + test)
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
