"""Mutation checks of method F `slsqp_fd` and its ablation queue (#1218).

Each mutation replaces one exact snippet in a private copy of the harness,
runs the fake-evaluator tests of `test_fd_gradient.py` (no FEM), and is
*killed* when a test fails with an assertion. The mutations the issue
names are marked [#1218]: an uncharged FD solve, and the backward-step and
refusal paths. The #1224 mutations of method F_w (base-started stencil
solves), its queue, session and recovery, and the `--warm` modes of the two
scripts are marked [#1224] where the issue names them.

    benchmarks/motor_optimization/harness/pyrun.sh \\
        benchmarks/motor_optimization/harness/fd_gradient_mutations.py > fd-gradient-checks.json
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
TESTS = ("FdGradientTests", "AblationQueueTests", "FdGradientCheckLevelTests",
         "FdStepSweepTests", "FdWarmTests", "FdWarmQueueTests", "FdWarmCheckTests")
MUTATIONS = (
    # ── [#1218] an uncharged FD solve ───────────────────────────────────
    ("fd_stencil_solve_uncharged [#1218]", "driver.py",
     'r = self._request("train", "forward", x_bits=to_bits(xs), gradient=False,',
     'r = self.ev.request("train", x_bits=to_bits(xs), gradient=False,'),
    ("fd_remesh_probe_uncharged [#1218]", "driver.py",
     'r = self._request("check", "mesh_motion", x_bits=to_bits(xs))',
     'r = self.ev.request("check", x_bits=to_bits(xs))'),
    ("fd_stencil_charged_as_line_search", "driver.py",
     'r = self._request("train", "forward", x_bits=to_bits(xs), gradient=False,\n'
     '                          purpose="iterate", note=note, **warm)',
     'r = self._request("train", "line_search", x_bits=to_bits(xs), gradient=False,\n'
     '                          purpose="line_search", note=note, **warm)'),
    ("fd_gradient_not_shared_by_objective_and_constraint", "driver.py",
     "    run._fd[key] = out\n", "\n"),
    # ── the gradient itself ─────────────────────────────────────────────
    ("f_requests_the_adjoint", "driver.py",
     "return run.train(u, gradient=not fd, purpose=purpose, note=\"slsqp\")",
     "return run.train(u, gradient=True, purpose=purpose, note=\"slsqp\")"),
    ("f_uses_the_adjoint_reply", "driver.py",
     '        if fd:\n            g = fd_gradient(run, u)',
     '        if False:\n            g = fd_gradient(run, u)'),
    ("fd_step_1e-5", "driver.py", "FD_STEP = 1e-6", "FD_STEP = 1e-5"),
    ("fd_divides_by_the_nominal_step_sign_lost", "driver.py",
     "            h = float(xs[k] - x[k])", "            h = FD_STEP"),
    ("fd_base_resolved_not_reused", "driver.py",
     '            dF[k] = (r["F"] - base["F"]) / h',
     '            dF[k] = (r["F"] - run.train(u, gradient=False)["F"] * (1 + 1e-9)) / h'),
    # ── [#1218] backward step ───────────────────────────────────────────
    ("fd_no_backward_step [#1218]", "driver.py",
     'for side, sign in (("forward", 1.0), ("backward", -1.0)):',
     'for side, sign in (("forward", 1.0),):'),
    ("fd_box_not_checked [#1218]", "driver.py",
     '        if k is not None and not self.problem.lower[k] <= xs[k] <= self.problem.upper[k]:',
     '        if False:'),
    ("fd_unbuildable_step_used [#1218]", "driver.py",
     '        if not g["buildable"]:\n            return "not_buildable"',
     '        if False:\n            return "not_buildable"'),
    ("fd_requires_admissible_stencil [#1218 ruling 1]", "driver.py",
     '        if not g["buildable"]:\n            return "not_buildable"',
     '        if not g["admissible"]:\n            return "not_buildable"'),
    ("fd_remesh_not_probed [#1218]", "driver.py",
     '        if r.get("trigger") is not None:\n            return "would_remesh"',
     '        if False:\n            return "would_remesh"'),
    ("fd_stencil_generation_unchecked", "driver.py",
     'if r["event"] is not None or r["generation"] != generation:',
     'if False:'),
    ("fd_retired_base_not_resolved", "driver.py",
     '    if base["generation"] != run.generation:',
     '    if False:'),
    # ── [#1218] refusal ─────────────────────────────────────────────────
    ("fd_refusal_wrong_kind [#1218]", "driver.py",
     '            run.refuse_evaluated(u, "derivative_undefined",\n',
     '            run.refuse_evaluated(u, "not_buildable",\n'),
    ("fd_refused_value_still_returned [#1218]", "driver.py",
     "        if fd and x.tobytes() in run._refused:",
     "        if False:"),
    ("fd_refused_base_is_its_own_anchor [#1218]", "driver.py",
     "        self.anchor_u = trail[-1].copy() if trail else None\n", "\n"),
    ("fd_value_request_at_undefined_derivative [#1218 review]", "driver.py",
     '            if g["buildable"] and not g["jacobian_defined"]:\n'
     '                run._refuse(u, x, "derivative_undefined", min_h=g["min_h"])',
     '            if False:\n'
     '                run._refuse(u, x, "derivative_undefined", min_h=g["min_h"])'),
    ("fd_undefined_derivative_refused_as_not_buildable", "driver.py",
     '                run._refuse(u, x, "derivative_undefined", min_h=g["min_h"])',
     '                run._refuse(u, x, "not_buildable", min_h=g["min_h"])'),
    ("fake_check_ignores_mesh_bad", "test_harness.py",
     "fires = gen != self.generation or self.mesh_bad(u)",
     "fires = gen != self.generation"),
    ("fd_stencil_is_an_optimizer_evaluation", "driver.py",
     '        self.generation = r["generation"]\n        event = dict(self._stamp(), kind="fd_train",',
     '        self.generation = r["generation"]\n        self._train.setdefault(np.asarray(xs).tobytes(), '
     'dict(self._train[next(iter(self._train))], x=np.asarray(xs)))\n'
     '        event = dict(self._stamp(), kind="fd_train",'),
    # ── the queues ──────────────────────────────────────────────────────
    ("f_added_to_main_registry", "driver.py",
     "MAIN_METHOD_SPECS = {m: dict(s) for m, s in METHOD_SPECS.items()}",
     "MAIN_METHOD_SPECS = METHOD_SPECS"),
    ("ablation_start_rotation_ignored", "fd_ablation_schedule.py",
     "    k = start_index % len(LETTERS)", "    k = 0"),
    ("ablation_repeat2_not_reversed", "fd_ablation_schedule.py",
     "return order if repeat == 1 else order[::-1]", "return order"),
    ("ablation_one_repeat", "fd_ablation_schedule.py",
     "    for repeat in (1, 2):", "    for repeat in (1,):"),
    ("ablation_f_on_adjoint_allowed", "fd_ablation_schedule.py",
     'or f.get("gradient") != "forward_difference"):', '):'),
    ("ablation_session_runs_the_main_queue", "session.py",
     '    if label == "fd_ablation":\n        return fd_ablation_schedule.registered_jobs(FD_ABLATION_SPECS)',
     '    if label == "fd_ablation":\n        return main_schedule.registered_jobs(MAIN_METHOD_SPECS)'),
    ("ablation_session_calibrates_l0", "session.py",
     "        if a.main:\n            l0_mesh", "        if not a.smoke:\n            l0_mesh"),
    ("ablation_session_allows_dirty_sources", "session.py",
     '        if main_like and recovery_inputs.get("source", {}).get("tree_clean") is not True:',
     '        if a.main and recovery_inputs.get("source", {}).get("tree_clean") is not True:'),
    ("ablation_recovery_mode_dropped", "recovery.py",
     '        mode = ("main" if args.main else "fd_ablation" if args.fd_ablation\n',
     '        mode = ("main" if args.main\n'),
    ("ablation_recovery_allows_dirty_sources", "recovery_bindings.py",
     'if session.get("label") in ("main", "fd_ablation", "fd_warm_ablation") and (',
     'if session.get("label") in ("main", "fd_warm_ablation") and ('),
    # ── fd_gradient_check.py --level ────────────────────────────────────
    ("fd_check_default_level_1", "fd_gradient_check.py",
     'choices=sorted(LEVELS), default=0,', 'choices=sorted(LEVELS), default=1,'),
    ("fd_check_level_not_passed_to_check", "fd_gradient_check.py",
     'check(a.binary, space, Path(work) / space, a.level, a.warm)',
     'check(a.binary, space, Path(work) / space, a.warm)'),
    ("fd_check_l1_trains_on_l0", "fd_gradient_check.py",
     '    1: {"train_mesh": MESHES / "icem_initial_sector_L1.msh",',
     '    1: {"train_mesh": MESHES / "icem_initial_sector_L0.msh",'),
    ("fd_check_l1_validates_at_l1", "fd_gradient_check.py",
     '"val_mesh": MESHES / "icem_initial_sector_L2.msh", "val_level": 2},',
     '"val_mesh": MESHES / "icem_initial_sector_L1.msh", "val_level": 1},'),
    ("fd_check_train_level_hard_coded", "fd_gradient_check.py",
     '"--train-level", str(level),', '"--train-level", "0",'),
    ("fd_check_l1_frozen_hash_wrong", "fd_gradient_check.py",
     '"c4526a26904c73f68ba0987f0ca8995bcc0095f0e7936399d5920e167f35d7f5"',
     '"c4526a26904c73f68ba0987f0ca8995bcc0095f0e7936399d5920e167f35d7f6"'),
    ("fd_check_mesh_hash_unchecked", "fd_gradient_check.py",
     '        if got != want:\n            raise SystemExit',
     '        if False:\n            raise SystemExit'),
    ("fd_check_report_hash_unchecked", "fd_gradient_check.py",
     'if json.loads(report.read_text())["msh_sha256"] != want:', 'if False:'),
    ("fd_check_meshes_not_verified", "fd_gradient_check.py",
     '        raise SystemExit(f"refusing to overwrite {a.out}")\n    verify_meshes(a.level)',
     '        raise SystemExit(f"refusing to overwrite {a.out}")'),
    # ── fd_step_sweep.py (#1218 diagnostic) ─────────────────────────────
    ("sweep_central_uses_the_forward_base", "fd_step_sweep.py",
     'lo = base if scheme == "forward" else stencil[("-", h)]', 'lo = base'),
    ("sweep_divides_by_the_nominal_step", "fd_step_sweep.py",
     'dx = plus["dx"] - (0.0 if scheme == "forward" else lo["dx"])', 'dx = h'),
    ("sweep_remesh_not_probed", "fd_step_sweep.py",
     'reason = run.fd_step_unusable(xs, k)', 'reason = None'),
    ("sweep_stencil_point_not_solved_fresh", "fd_step_sweep.py",
     '        if key not in solved:\n', '        if not solved:\n'),
    ("sweep_base_repeat_skipped", "fd_step_sweep.py",
     'again = run.fd_solve(x, generation, note="fd step sweep base repeat")',
     'again = dict(base, generation=generation)'),
    ("sweep_backward_step_is_forward", "fd_step_sweep.py",
     'value(k, shifted(k, -h), f"fd sweep {name} -{h:g}")',
     'value(k, shifted(k, h), f"fd sweep {name} -{h:g}")'),
    ("sweep_no_1e-7_forward_step", "fd_step_sweep.py",
     'FORWARD_STEPS = (1e-5, 1e-6, 1e-7)', 'FORWARD_STEPS = (1e-5, 1e-6)'),
    ("sweep_interior_point_moved", "fd_step_sweep.py",
     'return x0 + 0.25 * (centre - x0)', 'return x0 + 0.5 * (centre - x0)'),
    ("sweep_default_level_0", "fd_step_sweep.py",
     'choices=sorted(fdc.LEVELS), default=1)', 'choices=sorted(fdc.LEVELS), default=0)'),
    ("sweep_meshes_not_verified", "fd_step_sweep.py",
     '        raise SystemExit(f"refusing to overwrite {a.out}")\n    fdc.verify_meshes(a.level)',
     '        raise SystemExit(f"refusing to overwrite {a.out}")'),
    # ── [#1224] F_w: base-started stencil solves ────────────────────────
    ("w_stencils_cold [#1224]", "driver.py",
     "warm_from=x if run.fd_warm else None)", "warm_from=None)"),
    ("w_stencil_chains_from_the_previous_stencil [#1224]", "driver.py",
     "warm_from=x if run.fd_warm else None)", "warm_from=xs if run.fd_warm else None)"),
    ("w_base_not_retained [#1224]", "driver.py",
     'retain = {"retain_fields": True} if self.fd_warm else {}', "retain = {}"),
    ("w_retired_base_resolved_without_retain [#1224]", "driver.py",
     "                            retain=run.fd_warm)", "                            retain=False)"),
    ("w_spec_stencil_start_cold", "driver.py",
     'letter="W", fd_stencil_start="base",', 'letter="W", fd_stencil_start="cold",'),
    ("w_also_changes_f [#1224]", "driver.py",
     'self.fd_warm = METHOD_SPECS[method].get("fd_stencil_start") == "base"',
     'self.fd_warm = METHOD_SPECS[method].get("gradient") == "forward_difference"'),
    ("w_stencil_reply_unchecked", "driver.py",
     "        self._check_warm(r, warm)\n", "\n"),
    ("w_train_reply_unchecked", "driver.py",
     "        self._check_warm(r, retain)\n", "\n"),
    ("w_spec_added_to_main_registry", "driver.py",
     'MAIN_METHOD_SPECS = {m: dict(s) for m, s in METHOD_SPECS.items()}\n',
     'MAIN_METHOD_SPECS = {m: dict(s) for m, s in METHOD_SPECS.items()}\n'
     'MAIN_METHOD_SPECS["slsqp_fd_warm"] = dict(METHOD_SPECS["slsqp"], letter="W")\n'),
    ("w_queue_two_repeats [#1224]", "fd_warm_schedule.py", "REPEATS = (1,)", "REPEATS = (1, 2)"),
    ("w_queue_rotation_ignored", "fd_warm_schedule.py",
     "    k = start_index % len(LETTERS)", "    k = 0"),
    ("w_queue_accepts_cold_stencils", "fd_warm_schedule.py",
     '            or w.get("fd_stencil_start") != "base"):', '            ):'),
    ("w_session_runs_the_1218_queue", "session.py",
     '    if label == "fd_warm_ablation":\n        return fd_warm_schedule.registered_jobs(FD_WARM_SPECS)',
     '    if label == "fd_warm_ablation":\n        return fd_ablation_schedule.registered_jobs(FD_ABLATION_SPECS)'),
    ("w_session_not_main_like", "session.py",
     "    main_like = a.main or a.fd_ablation or a.fd_warm_ablation\n",
     "    main_like = a.main or a.fd_ablation\n"),
    ("w_session_block_without_repeats", "session.py",
     '                                   "repeats": [1], "post_hoc": True}',
     '                                   "post_hoc": True}'),
    ("w_recovery_mode_dropped", "recovery.py",
     '                else "fd_warm_ablation" if args.fd_warm_ablation else "smoke")',
     '                else "smoke")'),
    ("w_recovery_allows_dirty_sources", "recovery_bindings.py",
     'if session.get("label") in ("main", "fd_ablation", "fd_warm_ablation") and (',
     'if session.get("label") in ("main", "fd_ablation") and ('),
    ("w_check_accepts_cold_stencils", "fd_gradient_check.py",
     '        if c.get("start") != "same_mesh":', '        if False:'),
    ("w_check_gate_factor_100", "fd_gradient_check.py",
     "GATE_FACTOR = 10.0", "GATE_FACTOR = 100.0"),
    ("w_sweep_stencils_cold", "fd_step_sweep.py",
     "                xs, generation, note=note, warm_from=x if run.fd_warm else None)",
     "                xs, generation, note=note)"),
)


def main():
    sources = {p.name: p.read_text() for p in HERE.glob("*.py")}
    rows = []
    for name, file, before, after in (("baseline", None, None, None), *MUTATIONS):
        candidate = dict(sources)
        if file:
            if candidate[file].count(before) != 1:
                raise ValueError(f"{name}: expected exactly one mutation anchor in {file}")
            candidate[file] = candidate[file].replace(before, after)
            compile(candidate[file], file, "exec")
        with tempfile.TemporaryDirectory(prefix="motor-fd-gradient-") as td:
            for filename, source in candidate.items():
                (Path(td) / filename).write_text(source)
            result = subprocess.run([sys.executable, "-I", str(Path(td) / "test_fd_gradient.py"),
                                     *TESTS], capture_output=True, text=True, timeout=120)
        failed = sorted({line.split(" ")[1] for line in result.stderr.splitlines()
                         if line.startswith(("FAIL: ", "ERROR: "))})
        # Killed: a named test fails or errors (not an import failure).
        killed = (result.returncode != 0 and bool(failed)
                  and "Failed to import test module" not in result.stderr)
        expected = result.returncode == 0 if file is None else killed
        rows.append({"name": name, "expected_result": expected, "exit_code": result.returncode,
                     "mutated_file": file, "failing_tests": failed,
                     "stderr_tail": result.stderr[-1500:]})
    print(json.dumps({"issue": 1218, "issues": [1218, 1224], "python": sys.version,
                      "scope": ("fake-evaluator tests of method F and the {A', F} queue, and the "
                                "--level wiring of fd_gradient_check.py, and the fd_step_sweep.py "
                                "diagnostic; of method F_w, its {A', F_w} queue and session, and "
                                "the --warm modes of both scripts (#1224); no FEM"),
                      "tests": ["test_fd_gradient.py " + t for t in TESTS],
                      "baseline_source_sha256": {f: hashlib.sha256(s.encode()).hexdigest()
                                                 for f, s in sorted(sources.items())},
                      "mutations": len(MUTATIONS),
                      "killed": sum(r["expected_result"] for r in rows[1:]),
                      "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
