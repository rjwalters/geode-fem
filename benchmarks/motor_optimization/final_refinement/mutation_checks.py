"""Run selection tests against targeted semantic errors in isolated copies.

Writes the exact output as JSON to stdout. Never modifies the working tree.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "selection.py"
TESTS = HERE / "test_selection.py"
MUTATIONS = [
    ("mix_disjoint_rerun_ordinals", "slot = (r.method, r.space, r.start_index, r.repeat)",
     "slot = (r.method, r.space, r.start_index, r.repeat, r.ordinal)"),
    ("omit_mean_floor", "mean_floor = mean0 * 1.0025", "mean_floor = mean0"),
    ("omit_geometry", 'r.geometry_admissible is True, "geometry_not_admissible"',
     'True, "geometry_not_admissible"'),
    ("omit_mesh_policy", 'r.mesh_pass is True, "mesh_policy_not_passed"',
     'True, "mesh_policy_not_passed"'),
    ("omit_discrete_guard", 'r.discrete_pass is True, "discrete_guard_not_passed"',
     'True, "discrete_guard_not_passed"'),
    ("omit_contamination", 'r.host_contaminated is False, "host_contaminated_or_unknown"',
     'True, "host_contaminated_or_unknown"'),
    ("omit_completed", 'r.completed is True, "validation_not_completed"',
     'True, "validation_not_completed"'),
    ("credit_after_budget", "0 <= r.elapsed_seconds <= r.budget_seconds", "0 <= r.elapsed_seconds"),
    ("exclude_exact_budget", "0 <= r.elapsed_seconds <= r.budget_seconds", "0 <= r.elapsed_seconds < r.budget_seconds"),
    ("repeat2_substitution", 'r.repeat == 1 and audit[r.record_id]', 'audit[r.record_id]'),
    ("omit_error_margin", "r.f_val + max(0.1 * r.f_val, 2.5e-7)", "r.f_val"),
    ("omit_absolute_margin", "max(0.1 * r.f_val, 2.5e-7)", "0.1 * r.f_val"),
    ("crossing_by_objective", "lambda r: (r.elapsed_seconds, *r.tie)", "lambda r: (r.f_val, *r.tie)"),
    ("representative_by_time", 'group, lambda r: (r.f_val, *r.tie)',
     'group, lambda r: (r.elapsed_seconds, *r.tie)'),
    ("repeat_before_start", "return self.start_index, self.repeat, self.ordinal",
     "return self.repeat, self.start_index, self.ordinal"),
    ("drop_earlier_failed_run", 'r.eligible_run is True, "run_not_eligible"',
     'r.eligible_run is True and r.stop_reason != "failed", "run_not_eligible"'),
    ("rounded_geometry", 'struct.pack(">d", value).hex()', 'struct.pack(">d", round(value, 8)).hex()'),
    # #1218: the FD-ablation cohort, and the main cohort left unchanged.
    ("ablation_admits_main_methods", 'COHORTS = {"main": METHODS, "fd_ablation": ("A", "F")}',
     'COHORTS = {"main": METHODS, "fd_ablation": ("A", "B", "F")}'),
    ("ablation_roles_use_bare_letters", "labels = ARMS.get(cohort, {m: m for m in methods})",
     "labels = {m: m for m in methods}"),
    ("ablation_crossings_from_any_repeat", 'r.repeat == 1 and audit[r.record_id]',
     '(r.repeat == 1 or cohort != "main") and audit[r.record_id]'),
    ("main_output_gains_cohort_fields", 'if cohort != "main":\n        out.update(',
     'if True:\n        out.update('),
    ("main_cells_gain_arm_labels", 'if cohort != "main":\n                    cell["arm"]',
     'if True:\n                    cell["arm"]'),
    ("main_accepts_method_F", "        _metadata(r, methods)\n",
     '        _metadata(r, METHODS + ("F",))\n'),
    # #1224: the F_w cohort, and the main and #1218 cohorts left unchanged.
    ("warm_admits_method_F", 'COHORTS["fd_warm_ablation"] = ("A", "W")',
     'COHORTS["fd_warm_ablation"] = ("A", "F", "W")'),
    ("warm_roles_use_the_f_label", 'ARMS["fd_warm_ablation"] = {"A": "A_prime", "W": "F_w"}',
     'ARMS["fd_warm_ablation"] = {"A": "A_prime", "W": "F"}'),
    ("warm_accepts_repeat_2", "        if cohort in REPEAT1_ONLY and r.repeat != 1:\n",
     "        if False:\n"),
    ("fd_ablation_becomes_repeat1_only", 'REPEAT1_ONLY = ("fd_warm_ablation",)',
     'REPEAT1_ONLY = ("fd_warm_ablation", "fd_ablation")'),
    ("warm_output_claims_1218", 'preregistration=PREREGISTRATIONS.get(cohort))',
     'preregistration=PREREGISTRATIONS["fd_ablation"])'),
    ("fd_ablation_output_claims_1224", 'preregistration=PREREGISTRATIONS.get(cohort))',
     'preregistration=PREREGISTRATIONS["fd_warm_ablation"])'),
]


def main():
    source, tests = SOURCE.read_text(), TESTS.read_text()
    rows = []
    for name, before, after in [("baseline", None, None), *MUTATIONS]:
        candidate = source
        if before is not None:
            if source.count(before) != 1:
                raise RuntimeError(f"{name}: mutation anchor must occur exactly once")
            candidate = source.replace(before, after)
        compile(candidate, f"{name}/selection.py", "exec")
        with tempfile.TemporaryDirectory(prefix="motor-selector-") as td:
            root = Path(td)
            (root / "selection.py").write_text(candidate)
            (root / "test_selection.py").write_text(tests)
            # The #1218 check-plan test imports the (unmutated) planner.
            (root / "check_plan.py").write_text((HERE / "check_plan.py").read_text())
            p = subprocess.run([sys.executable, "-I", str(root / "test_selection.py")],
                               capture_output=True, text=True, timeout=30)
        passed = p.returncode == 0 if name == "baseline" else (
            p.returncode != 0 and "AssertionError" in p.stderr)
        rows.append({"name": name, "expected_result": passed, "exit_code": p.returncode,
                     "source_sha256": hashlib.sha256(candidate.encode()).hexdigest(),
                     "stdout": p.stdout, "stderr": p.stderr})
    print(json.dumps({"source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                      "tests_sha256": hashlib.sha256(tests.encode()).hexdigest(),
                      "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
