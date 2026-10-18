"""Mutation checks of the experiment-2 layer (issue #1193).

Each mutation replaces one exact snippet of
crates/geode-core/tests/common/icem_experiment2.rs, runs the CI target
`motor_experiment2`, records which tests fail, and restores the file. A
mutation is *killed* when at least one test fails. Run from the repository
root with CARGO_TARGET_DIR set:

    python3 -I benchmarks/motor_optimization/mutation_checks.py
"""
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates/geode-core/tests/common/icem_experiment2.rs"

MUTATIONS = [
    ("delta_rel_0.01", "pub const DELTA_F_REL: f64 = 0.10;", "pub const DELTA_F_REL: f64 = 0.01;"),
    ("delta_abs_2.5e-8", "pub const DELTA_F_ABS: f64 = 2.5e-7;", "pub const DELTA_F_ABS: f64 = 2.5e-8;"),
    ("delta_max_to_min", "(DELTA_F_REL * f_val).max(DELTA_F_ABS)", "(DELTA_F_REL * f_val).min(DELTA_F_ABS)"),
    ("delta_nan_guard_removed", "    if f_val.is_nan() {\n        return f64::NAN;\n    }\n", ""),
    ("margin_not_added", "let f_upper = f_val + d;", "let f_upper = f_val;"),
    ("target_le_to_lt", "let f_ok = f_upper <= target;", "let f_ok = f_upper < target;"),
    ("mean_condition_dropped", "attained: f_ok && mean_ok,", "attained: f_ok,"),
    ("mean_ge_to_gt", "let mean_ok = mean_full >= mean_floor;", "let mean_ok = mean_full > mean_floor;"),
    ("floor_margin_2.5pct", "pub const MEAN_FLOOR_MARGIN: f64 = 0.0025;", "pub const MEAN_FLOOR_MARGIN: f64 = 0.025;"),
    ("floor_without_margin", "self.mean0 * (1.0 + MEAN_FLOOR_MARGIN)", "self.mean0"),
    ("t90_fraction_0.2", '("T90", 0.1)', '("T90", 0.2)'),
    ("anchor_json_target_check_removed", "            if !close(v[\"targets\"][name].as_f64(), t) {", "            if false && !close(v[\"targets\"][name].as_f64(), t) {"),
    ("frozen_check_removed", "if v.to_bits() != f.to_bits() {", "if false && v.to_bits() != f.to_bits() {"),
    ("embed_bounds_check_removed", "            if !(self.lower[k]..=self.upper[k]).contains(&v) {", "            if false && !(self.lower[k]..=self.upper[k]).contains(&v) {"),
    ("gradient_restrict_shifted", "        self.free.iter().map(|&i| full[i]).collect()", "        self.free.iter().map(|&i| full[(i + 1) % full.len()]).collect()"),
    ("conforming_on_moved_mesh", "let need_fresh = trigger.is_some() || (arkkio && !own);", "let need_fresh = trigger.is_some();"),
    ("stator_offset_guard_removed", "if arkkio && !Sp::stator_offsets_zero(&geo) {", "if false && arkkio && !Sp::stator_offsets_zero(&geo) {"),
    ("policy_ignored", "(Some(q), self.cfg.eval.policy.trigger(&q))", "(Some(q), None)"),
    ("fresh_mesh_not_checked", "let tf = self.cfg.eval.policy.trigger(&quality);", "let tf = None::<RemeshTrigger>;"),
    ("level_check_removed", "        if n != expected {", "        if false && n != expected {"),
    ("primary_check_removed", "if !self.cfg.is_primary_model() || self.cfg.eval.level != anchor.level {", "if false {"),
    ("line_search_to_forward", "Purpose::LineSearch => CostCategory::LineSearch,", "Purpose::LineSearch => CostCategory::Forward,"),
    ("adjoint_not_charged", "            self.charge_time(CostCategory::Adjoint, adj, (0, n), note);", ""),
    ("validation_remesh_not_charged", "        if matches!(v.mesh, ValidationMesh::Fresh { .. }) {", "        if false && matches!(v.mesh, ValidationMesh::Fresh { .. }) {"),
    ("negative_charge_allowed", "            seconds.is_finite() && seconds >= 0.0 && fe.is_finite() && fe >= 0.0,", "            true,"),
    ("angle_grid_check_removed", "            if (kf - kf.round()).abs() > 1e-9 {", "            if false {"),
    # From the #1194 review (J1-J4, J6, J7) and the anchor-mesh check.
    ("J1_delta_at_anchor_F0", "    let d = delta_f(f_val);\n", "    let d = delta_f(anchor.f0);\n"),
    ("J2_full_current_is_half", "            .find(|c| c.fraction == 1.0)\n", "            .find(|c| c.fraction == 0.5)\n"),
    ("J3_fresh_solve_on_base_mesh", "let (currents, solve_seconds) = forward_on::<Sp>(&self.cfg, &fresh, p)?;", "let (currents, solve_seconds) = forward_on::<Sp>(&self.cfg, &self.base, p)?;"),
    ("J4_exhausted_strict", "        self.total_fe() >= self.budget_fe\n", "        self.total_fe() > self.budget_fe\n"),
    ("J6_validation_fe_uses_total_seconds", "            v.solve_seconds,\n            (v.solves(), 0),", "            v.seconds,\n            (v.solves(), 0),"),
    ("J7_is_primary_ignores_newton_tol", "            && self.eval.newton.rel_tol == p.eval.newton.rel_tol\n", "\n"),
    ("anchor_mesh_check_removed", "        if self.base_sha256 != anchor.mesh_sha256 {", "        if false && self.base_sha256 != anchor.mesh_sha256 {"),
    ("anchor_from_moved_mesh_allowed", "        if !matches!(v.mesh, ValidationMesh::Own { .. }) {", "        if false && !matches!(v.mesh, ValidationMesh::Own { .. }) {"),
    ("validation_sha_is_base_always", "            ValidationMesh::Fresh { sha256, .. } => sha256.clone(),", "            ValidationMesh::Fresh { .. } => self.base_sha256.clone(),"),
]


def run_tests():
    try:
        p = subprocess.run(
            ["cargo", "test", "-p", "geode-core", "--release", "--test", "motor_experiment2"],
            cwd=ROOT, capture_output=True, text=True, timeout=900)
    except subprocess.TimeoutExpired:
        return -1, ["(timeout after 900 s)"], False
    out = p.stdout + p.stderr
    failed = sorted(set(re.findall(r"^test (\S+)(?: - should panic)? \.\.\. FAILED", out, re.M)))
    compile_error = "error[" in out or "could not compile" in out
    return p.returncode, failed, compile_error


original = SRC.read_text()
try:
    rc, failed, ce = run_tests()
    print(f"baseline: exit {rc}, failed {failed}", flush=True)
    assert rc == 0 and not failed, "baseline must pass"
    killed = 0
    for name, old, new in MUTATIONS:
        assert original.count(old) == 1, f"{name}: snippet occurs {original.count(old)} times"
        SRC.write_text(original.replace(old, new))
        rc, failed, ce = run_tests()
        SRC.write_text(original)
        status = "KILLED" if (rc != 0) else "SURVIVED"
        killed += rc != 0
        why = "compile error" if ce else ", ".join(failed)
        print(f"{name}: {status} ({why})", flush=True)
    print(f"{killed}/{len(MUTATIONS)} mutations killed")
finally:
    SRC.write_text(original)
