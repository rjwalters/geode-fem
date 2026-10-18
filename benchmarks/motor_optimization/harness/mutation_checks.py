"""Mutation checks of the experiment-2 harness wiring (issues #1195, #1201,
#1197).

Each mutation replaces one exact snippet of a harness file, runs the tests
that bind it, records which tests fail, and restores the file. A mutation
is *killed* when at least one test fails. Driver mutations run the Python
tests (`test_harness.py` and `test_safeguards.py`, this environment);
evaluator mutations run the CI target `motor_experiment2_harness`, domain
mutations the CI target `motor_experiment2_domain` and the `icem_domain`
library tests, and remesh-loop mutations the `design_remesh` library tests
(cargo, release; set CARGO_TARGET_DIR). From the repository root:

    benchmarks/motor_optimization/harness/pyrun.sh \\
        benchmarks/motor_optimization/harness/mutation_checks.py

The four the issue names are marked [#1195]: wrong target, delta_F at the
wrong point, uncharged validation, validation on the wrong mesh. The rest
bind the budget and the charging wiring.
"""
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
RS = ROOT / "crates/geode-core/tests/common/icem_harness.rs"
DRV = HERE / "driver.py"
ACC = HERE / "acceptance.py"
LED = HERE / "ledger.py"
RUN = HERE / "run.py"
SES = HERE / "session.py"
DOM = ROOT / "crates/geode-core/src/mesh/icem_domain.rs"
LOOP = ROOT / "crates/geode-core/src/mesh/design_remesh.rs"
REM = ROOT / "crates/geode-core/tests/common/icem_remesh.rs"
LIN = HERE / "linalg_info.py"
TSF = HERE / "test_safeguards.py"
BDM = ROOT / "benchmarks/motor_shape_gradient/design/build_design_mesh.py"

MUTATIONS = [
    # ── [#1195] wrong target ────────────────────────────────────────────
    ("wrong_target_recorded [#1195]", DRV,
     "if name not in self.attained and flags[name]:",
     'if name not in self.attained and flags["T50"]:'),
    ("wrong_target_fraction_T90 [#1195]", ACC,
     '("T90", 0.1)', '("T90", 0.2)'),
    # ── [#1195] delta_F at the wrong point ──────────────────────────────
    ("delta_f_at_anchor_F0 [#1195]", ACC,
     "    d = delta_f(f_val)\n", "    d = delta_f(anchor.f0)\n"),
    ("delta_f_at_training_value [#1195]", DRV,
     'flags, values = check_acceptance(self.anchor, r["F_val"], r["mean_full"],',
     'flags, values = check_acceptance(self.anchor, self._train[key]["F"] '
     'if key in self._train else r["F_val"], r["mean_full"],'),
    # ── [#1195] uncharged validation ────────────────────────────────────
    ("validation_uncharged_evaluator [#1195]", RS,
     "        ledger.charge_validation(&v, note);\n", "\n"),
    ("validation_uncharged_driver [#1195]", DRV,
     '        for c in reply.get("charges", []):',
     '        for c in (reply.get("charges", []) if op != "validate" else []):'),
    ("validation_abort_charged_elsewhere [#1195]", DRV,
     'r = self._request("validate", "validation", x_bits=to_bits(x), note=note)',
     'r = self._request("validate", "forward", x_bits=to_bits(x), note=note)'),
    # ── [#1195] validation on the wrong mesh ────────────────────────────
    ("validator_on_training_mesh [#1195]", RS,
     "Validator::<Sp>::new(val_msh, theta0, reference, cfg.validation(), val_source)",
     "Validator::<Sp>::new(train_msh, theta0, reference, cfg.validation(), val_source)"),
    ("anchor_mesh_startup_check_removed [#1195]", RS,
     "            if a.mesh_sha256 != validator.base_sha256 {",
     "            if false && a.mesh_sha256 != validator.base_sha256 {"),
    ("driver_ready_mesh_check_removed [#1195]", DRV,
     'if ready.get("validation_msh_sha256") != self.anchor.mesh_sha256:',
     "if False:"),
    ("driver_reply_mesh_check_removed [#1195]", DRV,
     'if r["validator_msh_sha256"] != self.anchor.mesh_sha256 \\',
     'if False and r["validator_msh_sha256"] != self.anchor.mesh_sha256 \\'),
    # ── budget ──────────────────────────────────────────────────────────
    ("budget_edge_strict", LED,
     "return self._seconds >= self.budget_seconds",
     "return self._seconds > self.budget_seconds"),
    ("credited_edge_strict", DRV,
     "credited = self.ledger.total_fe() <= self.ledger.budget_fe",
     "credited = self.ledger.total_fe() < self.ledger.budget_fe"),
    ("attainment_beyond_budget_credited", DRV,
     "        if credited:\n            for name in TARGET_NAMES:",
     "        if True:\n            for name in TARGET_NAMES:"),
    ("abort_remainder_not_charged", DRV,
     '            led.charge_to_budget(category, note=f"{op} aborted at the budget")',
     "            pass"),
    ("no_deadline_on_requests", DRV,
     "reply = self.ev.request(op, timeout=led.remaining_seconds(), **fields)",
     "reply = self.ev.request(op, timeout=None, **fields)"),
    ("exhausted_not_checked_before_request", DRV,
     "        if led.exhausted():\n            raise BudgetExhausted()\n        x_req =",
     "        x_req ="),
    ("time_after_the_stop_charged", RUN,
     "        if not ledger.exhausted():\n            # Up to the stop;",
     "        if True:\n            # Up to the stop;"),
    ("t99_does_not_stop", DRV,
     '        if "T99" in self.attained:\n            raise TargetReached()\n', ""),
    # ── charging ────────────────────────────────────────────────────────
    ("overhead_not_charged", LED,
     "        if gap > 0:\n            self.charge(\"overhead\", gap, note=note)",
     "        if False:\n            pass"),
    ("driver_time_not_charged", LED,
     "        self.charge(category, max(own, 0.0), note=note)\n        return out",
     "        return out"),
    ("line_search_labelled_iterate_driver", DRV,
     'purpose = "iterate" if state["after_gradient"] else "line_search"',
     'purpose = "iterate"'),
    ("line_search_labelled_iterate_evaluator", RS,
     '"line_search" => Ok(Purpose::LineSearch),',
     '"line_search" => Ok(Purpose::Iterate),'),
    ("failure_not_charged", RS,
     '                    ledger.charge_time(cat, uncovered, (0, 0), &format!("failed: {e}"));',
     "                    let _ = (cat, uncovered);"),
    ("unit_is_the_mean", RS,
     "        let unit = median(&val_s);",
     "        let unit = val_s.iter().sum::<f64>() / val_s.len() as f64;"),
    ("mean_full_is_half_current", RS,
     '            "mean_full": ev.means[k],', '            "mean_full": ev.means[1],'),
    ("dmean_full_is_half_current", RS,
     '"dmean_full": ev.d_means.as_ref().map(|d| d[k].clone()),',
     '"dmean_full": ev.d_means.as_ref().map(|d| d[1].clone()),'),
    # ── Judge round 1 (J3, J6) and amendments 4-6 ───────────────────────
    ("J3_driver_x_bits_echo_check_removed", DRV,
     'if "x_bits" in fields and reply.get("x_bits") != fields["x_bits"]:', "if False:"),
    ("J6_attained_cross_check_removed", ACC,
     '        if got[name]["attained"] != flags[name]:', "        if False:"),
    ("start_validated", DRV,
     "        if key == self.start_key or key in self._val or key not in self._train:",
     "        if key in self._val or key not in self._train:"),
    ("cobyqa_validates_inside_its_initial_set", DRV,
     "        if len(seen) < COBYQA_INITIAL(P.n):", "        if False:"),
    ("c1_geometry_substitute_not_a_rejection", DRV,
     "        else:\n            # A rejection, including a non-improving geometry point",
     "        elif not geometry:\n            # A rejection, including a non-improving geometry point"),
    ("unit_is_median_of_process_medians", SES,
     "    unit = median(val)\n",
     '    unit = median([median(c["validation_seconds"]) for c in cals])\n'),
    ("diagnose_leaves_training_currents_changed", RS,
     "        self.train.cfg.fractions = saved;\n", "\n"),
    ("diagnose_leaves_validation_currents_changed", RS,
     "        self.validator.cfg.eval.fractions = saved;\n", "\n"),
    ("method_order_not_alternating", DRV,
     'letters = "ABC" if repeat % 2 == 1 else "CBA"', 'letters = "ABC"'),
    # ── #1197 (K2, K4) ──────────────────────────────────────────────────
    ("K2_cobyqa_initial_set_gate_one_evaluation_late [#1197]", DRV,
     "        if len(seen) < COBYQA_INITIAL(P.n):", "        if len(seen) <= COBYQA_INITIAL(P.n):"),
    ("K2_c_initial_set_incumbent_not_validated_at_once [#1197]", DRV,
     "    if inc is not pts[0]:\n        run.accept_iterate(inc[\"u\"], note=\"surrogate doe incumbent\",",
     "    if False:\n        run.accept_iterate(inc[\"u\"], note=\"surrogate doe incumbent\","),
    ("K4_aborted_request_not_recorded [#1197]", DRV,
     '            self.aborted_request = {"category": category, "op": op, "x": x_req}',
     "            pass"),
    # ── #1201: amendment 7 §1 (domain) ──────────────────────────────────
    ("strict_guard_passes_at_zero [#1201]", DOM,
     "        Some(v) if spec.strict => v > 0.0,", "        Some(v) if spec.strict => v >= 0.0,"),
    ("tau_is_not_0.05 [#1201]", DOM,
     "pub const TAU: f64 = 0.05;", "pub const TAU: f64 = 0.04;"),
    ("derivative_invented_at_zero_discriminant [#1201]", DOM,
     "(!singular && g.iter().all(|x| x.is_finite())).then_some(g)", "Some(g)"),
    ("downstream_margin_not_guarded [#1201]", DOM,
     "        if !(disc.value().is_nan() || disc.value() < 0.0) {",
     "        if !disc.value().is_nan() {"),
    ("evaluator_trains_a_non_buildable_point [#1201]", RS,
     '        let d = self.domain_at(&x, ledger, "buildability")?;\n        Self::require_buildable(&d)?;\n        if gradient',
     '        let d = self.domain_at(&x, ledger, "buildability")?;\n        if gradient'),
    ("evaluator_gradient_at_undefined_derivative [#1201]", RS,
     "        if gradient && !d.jacobian_defined() {", "        if false {"),
    ("evaluator_credit_without_admissibility [#1201]", RS,
     "        let a = a.with_admissibility(d.credit_admissible());", "        let a = a;"),
    ("evaluator_credit_tolerance_is_zero [#1201]", DOM,
     "pub const CREDIT_H_TOLERANCE: f64 = 1e-12;", "pub const CREDIT_H_TOLERANCE: f64 = 0.0;"),
    ("driver_credit_tolerance_is_zero [#1201]", ACC,
     "CREDIT_H_TOLERANCE = 1e-12", "CREDIT_H_TOLERANCE = 0.0"),
    ("driver_credit_tolerance_too_wide [#1201]", ACC,
     "CREDIT_H_TOLERANCE = 1e-12", "CREDIT_H_TOLERANCE = 1e-6"),
    # ── #1201: credit (adoption correction 2) ───────────────────────────
    ("driver_credit_without_admissibility [#1201]", ACC,
     "        flags[name] = (upper <= t) and mean_ok and admissible is True",
     "        flags[name] = (upper <= t) and mean_ok"),
    ("best_feasible_without_admissibility [#1201]", DRV,
     '    feasible = [v for v in credited if v["mean_ok"] and v["admissible"]]',
     '    feasible = [v for v in credited if v["mean_ok"]]'),
    ("c_ordering_ignores_geometry_violation [#1201]", DRV,
     "    return max(0.0, -P.constraint(rec)) + float(np.sum(np.maximum(0.0, -h)))",
     "    return max(0.0, -P.constraint(rec))"),
    ("start_admissibility_not_checked [#1201]", DRV,
     '        if not g["admissible"]:\n            raise RestorationFailed("the start is not admissible")',
     '        if False:\n            raise RestorationFailed("the start is not admissible")'),
    # ── #1201: amendment 7 §2 (safeguards) ──────────────────────────────
    ("non_buildable_point_reaches_fem [#1201]", DRV,
     '        if not g["buildable"]:\n            self._refuse(u, x, "not_buildable"',
     '        if False:\n            self._refuse(u, x, "not_buildable"'),
    ("anchor_is_last_buildable_not_admissible [#1201]", DRV,
     '        if g["admissible"]:\n            # The restoration anchor',
     '        if g["buildable"]:\n            # The restoration anchor'),
    ("slsqp_restarts_at_the_anchor_not_y [#1201]", DRV,
     "            u = y\n            continue", "            u = run.anchor_u\n            continue"),
    ("bisections_unbounded [#1201]", DRV,
     "        for k in range(1, MAX_BISECTIONS + 1):", "        for k in range(1, MAX_BISECTIONS + 9):"),
    ("restarts_unbounded [#1201]", DRV,
     "            if len(run.restarts) >= MAX_RESTARTS:", "            if False:"),
    ("mesh_restoration_without_mesh_check [#1201]", DRV,
     '            if ok and refused.kind == "mesh_infeasible":',
     '            if False and refused.kind == "mesh_infeasible":'),
    ("cobyqa_barrier_not_nan [#1201]", DRV,
     "        return math.nan if r is None else P.objective(r)",
     "        return 1e6 if r is None else P.objective(r)"),
    ("c_no_minus_sigma_retry [#1201]", DRV,
     "        for step in (primary, -primary):", "        for step in (primary,):"),
    ("c_halvings_not_3 [#1201]", DRV, "C_HALVINGS = 3", "C_HALVINGS = 2"),
    ("c_refused_candidate_not_a_rejection [#1201]", DRV,
     "            accepted = False\n        else:\n            held_out(",
     "            continue\n        else:\n            held_out("),
    ("c_refusal_skips_the_geometry_point [#1201]", DRV,
     "            accepted = False\n        else:\n            held_out(",
     "            delta *= 0.5\n            rejections += 1\n            continue\n"
     "        else:\n            held_out("),
    # ── #1201: amendment 7 §3 (mesh repair) ─────────────────────────────
    ("repair_attempts_cumulative [#1201]", REM,
     '        options: &[("Mesh.MeshSizeFactor", 0.9)],',
     '        options: &[("Mesh.Optimize", 1.0), ("Mesh.OptimizeNetgen", 1.0), ("Mesh.MeshSizeFactor", 0.9)],'),
    ("registered_session_without_repair [#1201]", RS,
     "            train_policy: RemeshPolicy::default(),\n            mesh_repair: true,",
     "            train_policy: RemeshPolicy::default(),\n            mesh_repair: false,"),
    ("fatal_attempt_history_dropped [#1201]", LOOP,
     "                    attempts.push(record(AttemptOutcome::Fatal(message.clone()), 0.0));\n                    return Err(RemeshError::Remesh {",
     "                    return Err(RemeshError::Remesh {"),
    ("mesh_infeasible_attempts_not_charged [#1201]", RS,
     "                ledger.charge_time(CostCategory::MeshMotion, motion, (0, 0), note);\n                ledger.charge_time(CostCategory::Remesh, remesh, (0, 0), note);",
     "                ledger.charge_time(CostCategory::MeshMotion, motion, (0, 0), note);"),
    # ── #1201: amendment 7 §4 (checkpoint) and §6 (host) ────────────────
    ("checkpoint_credits_work_in_flight [#1201]", DRV,
     '                          if v["credited"] and v["wall_fe"] <= fe and v["attained"][name]),',
     '                          if v["credited"] and v["wall_fe"] - 5.0 <= fe and v["attained"][name]),'),
    ("host_contamination_factor_not_1.5 [#1201]", SES,
     "CONTAMINATION_FACTOR = 1.5", "CONTAMINATION_FACTOR = 2.0"),
    ("host_pre_run_limit_not_n_plus_1 [#1201]", SES,
     "        if v <= procs + 1:", "        if v <= procs + 2:"),
    ("idle_gate_not_a_gate [#1201]", SES,
     '    if not passed:\n        return dict(base, status="deferred"',
     '    if False:\n        return dict(base, status="deferred"'),
    ("contamination_over_the_diagnostic_too [#1201]", SES,
     '    bad, longest = contaminated(samples, r0, r1, ctx["procs"])',
     '    bad, longest = contaminated(samples, r0, res.get("post_run_end_time", t1), ctx["procs"])'),
    ("calibration_not_gated [#1201]", SES,
     "        if passed:\n            break\n    else:", "        break\n    else:"),
    ("host_idle_ignores_contamination [#1201]", SES,
     '        if done[0]["host_contaminated"] and not any(',
     '        if False and not any('),
    # ── #1201: the #1200 Judge's gaps and the record ────────────────────
    ("training_remesh_hash_not_in_trace [#1201]", DRV,
     '                                remesh_meshes=r.get("remesh_meshes") or [],',
     "                                remesh_meshes=[],"),
    ("evaluator_remesh_hash_not_in_reply [#1201]", RS,
     '            "remesh_meshes": self.train_meshes_since(n0),\n            "quality"',
     '            "remesh_meshes": Vec::<Value>::new(),\n            "quality"'),
    ("failed_request_point_not_recorded [#1201]", DRV,
     '                                    x=self._x_from_bits(fields.get("x_bits")),',
     "                                    x=None,"),
    ("transport_failure_not_recorded [#1201]", DRV,
     '            self.events.append(dict(self._stamp(), kind="failed", op=op, x=x_req,',
     '            self.events.append(dict(self._stamp(), kind="failed", op=op, x=None,'),
    ("aborted_request_drops_x [#1201]", DRV,
     '            self.aborted_request = {"category": category, "op": op, "x": x_req}',
     '            self.aborted_request = {"category": category, "op": op}'),
    ("in_run_wall_includes_post_run [#1201]", RUN,
     '    trace["in_run_wall_seconds"] = trace["ledger"]["total_seconds"]',
     '    trace["in_run_wall_seconds"] = trace["ledger"]["total_seconds"] + (trace.get("post_run_diagnostic") or {}).get("seconds", 0.0)'),
    # ── #1204 Judge (J1-J8) ─────────────────────────────────────────────
    ("J1_anchor_uses_the_credit_tolerance [#1204]", DRV,
     '        if g["admissible"]:\n            # The restoration anchor',
     '        if g["credit_admissible"]:\n            # The restoration anchor'),
    ("J2_restored_y_only_buildable [#1204]", DRV,
     'ok = g["admissible"] and (g["jacobian_defined"] or not need_jacobian)',
     'ok = g["buildable"] and (g["jacobian_defined"] or not need_jacobian)'),
    ("J3_c_violation_drops_the_mean_floor [#1204]", DRV,
     "    return max(0.0, -P.constraint(rec)) + float(np.sum(np.maximum(0.0, -h)))",
     "    return float(np.sum(np.maximum(0.0, -h)))"),
    ("J4_discrete_failure_not_infinite [#1204]", DRV,
     '    if not rec.get("discrete_pass", True):\n        return math.inf',
     '    if not rec.get("discrete_pass", True):\n        return 0.0'),
    ("J5_driver_credit_boundary_strict [#1204]", DRV,
     'all(h >= -CREDIT_H_TOLERANCE for h in g["h"])',
     'all(h > -CREDIT_H_TOLERANCE for h in g["h"])'),
    ("J6_deferred_job_never_requeued [#1204]", SES,
     'if r["status"] == "deferred" and n + 1 < max_deferrals:',
     'if False and r["status"] == "deferred" and n + 1 < max_deferrals:'),
    ("J7_deferred_but_spawned [#1204]", SES,
     '    if not passed:\n        return dict(base, status="deferred", exit=None, host_contaminated=None)',
     '    if not passed:\n        spawn(["true"])\n        return dict(base, status="deferred", exit=None, host_contaminated=None)'),
    ("J8_c_refused_initial_step_counts [#1204]", DRV,
     "            if r is not None:\n                break", "            break"),
    ("G1_same_stack_always_false [#1204]", LIN,
     "    return all(a.get(k) == b.get(k) for k in REPLAY_KEYS)", "    return False"),
    ("G2_gate_ignores_blas_lapack [#1204]", LIN,
     '''REPLAY_KEYS = ("system", "machine", "macos_version", "numpy", "scipy",
               "numpy_blas", "numpy_blas_version", "numpy_lapack", "numpy_lapack_version",
               "scipy_blas", "scipy_blas_version", "scipy_lapack", "scipy_lapack_version")''',
     'REPLAY_KEYS = ("system", "machine", "macos_version", "numpy", "scipy")'),
    ("G2b_gate_ignores_the_macos_version [#1204]", LIN,
     'REPLAY_KEYS = ("system", "machine", "macos_version", "numpy", "scipy",',
     'REPLAY_KEYS = ("system", "machine", "numpy", "scipy",'),
    ("G3_mismatch_returns_instead_of_skipping [#1204]", TSF,
     '            self.skipTest("bit-for-bit replay holds only',
     '            return\n            self.skipTest("bit-for-bit replay holds only'),
    ("R3_rust_mesh_failure_exit_not_the_scripts [#1204]", REM,
     "pub const MESH_FAILURE_EXIT: i32 = 3;", "pub const MESH_FAILURE_EXIT: i32 = 4;"),
    ("R3b_script_mesh_failure_exit_not_rusts [#1204]", BDM,
     "\nMESH_FAILURE_EXIT = 3\n", "\nMESH_FAILURE_EXIT = 4\n"),
    ("R3c_script_marker_not_rusts [#1204]", BDM,
     '\nMESH_FAILURE_MARKER = "MESH_FAILURE"\n', '\nMESH_FAILURE_MARKER = "MESH_FAILED"\n'),
    ("mesher_exit_1_is_a_meshing_failure [#1204]", REM,
     "    if code == Some(MESH_FAILURE_EXIT) && log.contains(MESH_FAILURE_MARKER) {",
     "    if code.is_some() {"),
    ("validation_record_without_geometry_evidence [#1201]", DRV,
     'h_nonnegative=g["admissible"], geometry=g["detail"], **values)',
     'h_nonnegative=g["admissible"], geometry=None, **values)'),
]


def run_python():
    rc, failed, crashed = 0, [], False
    for t in ("test_harness.py", "test_safeguards.py"):
        try:
            # The suites take seconds; a mutant that loops is killed by
            # the timeout.
            p = subprocess.run([sys.executable, "-I", str(HERE / t)], cwd=ROOT,
                               capture_output=True, text=True, timeout=180)
        except subprocess.TimeoutExpired:
            return -1, failed + [f"({t} timeout)"], False
        out = p.stdout + p.stderr
        f = sorted(set(re.findall(r"^(test_\w+) \(.*\) \.\.\. (?:FAIL|ERROR)", out, re.M)))
        rc = rc or p.returncode
        failed += f
        crashed = crashed or ("Traceback" in out and not f)
    return rc, failed, crashed


def cargo(*args):
    try:
        p = subprocess.run(["cargo", "test", "-p", "geode-core", "--release", *args], cwd=ROOT,
                           capture_output=True, text=True, timeout=1800)
    except subprocess.TimeoutExpired:
        return -1, ["(timeout)"], False
    out = p.stdout + p.stderr
    failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", out, re.M)))
    return p.returncode, failed, "error[" in out or "could not compile" in out


def run_rust():
    """The evaluator target and the domain target (the evaluator's
    refusals are bound in both)."""
    return cargo("--test", "motor_experiment2_harness", "--test", "motor_experiment2_domain")


def run_domain():
    """The domain target and the icem_domain library tests."""
    rc, failed, ce = cargo("--test", "motor_experiment2_domain")
    rc2, failed2, ce2 = cargo("--lib", "mesh::icem_domain")
    return rc or rc2, failed + failed2, ce or ce2


def run_loop():
    """The design_remesh library tests and the evaluator target."""
    rc, failed, ce = cargo("--lib", "mesh::design_remesh")
    if rc == 0:
        return run_rust()
    return rc, failed, ce


def run_remesher():
    """The domain target (the attempt sequence) and the evaluator target."""
    rc, failed, ce = cargo("--test", "motor_experiment2_domain")
    if rc == 0:
        return run_rust()
    return rc, failed, ce


RUNNERS = {RS: run_rust, DOM: run_domain, LOOP: run_loop, REM: run_remesher,
           BDM: run_remesher}


def main(argv=None):
    """All mutations, or `--only NAME [NAME ...]` (a spot check; the
    committed log is always a full run)."""
    argv = sys.argv[1:] if argv is None else argv
    muts = MUTATIONS
    if argv[:1] == ["--only"]:
        muts = [m for m in MUTATIONS if m[0] in set(argv[1:])]
    originals = {f: f.read_text() for f in {m[1] for m in MUTATIONS}}
    try:
        for name, runner in (("python", run_python), ("rust", run_rust),
                             ("domain", run_domain), ("loop", run_loop)):
            rc, failed, _ = runner()
            print(f"baseline {name}: exit {rc}, failed {failed}", flush=True)
            assert rc == 0 and not failed, f"{name} baseline must pass"
        killed = 0
        for name, path, old, new in muts:
            src = originals[path]
            assert src.count(old) == 1, f"{name}: snippet occurs {src.count(old)} times"
            path.write_text(src.replace(old, new))
            try:
                rc, failed, compile_error = RUNNERS.get(path, run_python)()
            finally:
                path.write_text(src)
            ok = rc != 0
            killed += ok
            why = "compile error" if compile_error else ", ".join(failed)
            where = {RS: "rust", DOM: "domain", LOOP: "loop", REM: "remesher",
                     BDM: "remesher"}.get(path,
                                                                                "python")
            print(f"{name} ({where}): {'KILLED' if ok else 'SURVIVED'} ({why})", flush=True)
        print(f"{killed}/{len(muts)} mutations killed")
        return 0 if killed == len(muts) else 1
    finally:
        for f, src in originals.items():
            f.write_text(src)


if __name__ == "__main__":
    sys.exit(main())
