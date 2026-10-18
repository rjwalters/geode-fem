"""Mutation checks of the experiment-2 harness wiring (issue #1195).

Each mutation replaces one exact snippet of a harness file, runs the tests
that bind it, records which tests fail, and restores the file. A mutation
is *killed* when at least one test fails. Driver mutations run the Python
tests (`test_harness.py`, this environment); evaluator mutations run the CI
target `motor_experiment2_harness` (cargo, release; set CARGO_TARGET_DIR).
From the repository root:

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
     "        if led.exhausted():\n            raise BudgetExhausted()\n        try:",
     "        try:"),
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
     "        self.charge(category, self.clock() - t, note=note)\n        return out",
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
    ("method_order_not_alternating", DRV,
     'letters = "ABC" if repeat % 2 == 1 else "CBA"', 'letters = "ABC"'),
]


def run_python():
    p = subprocess.run([sys.executable, "-I", str(HERE / "test_harness.py")], cwd=ROOT,
                       capture_output=True, text=True, timeout=600)
    out = p.stdout + p.stderr
    failed = sorted(set(re.findall(r"^(test_\w+) \(.*\) \.\.\. (?:FAIL|ERROR)", out, re.M)))
    return p.returncode, failed, "Traceback" in out and not failed


def run_rust():
    try:
        p = subprocess.run(["cargo", "test", "-p", "geode-core", "--release", "--test",
                            "motor_experiment2_harness"], cwd=ROOT, capture_output=True,
                           text=True, timeout=1800)
    except subprocess.TimeoutExpired:
        return -1, ["(timeout)"], False
    out = p.stdout + p.stderr
    failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", out, re.M)))
    return p.returncode, failed, "error[" in out or "could not compile" in out


def main():
    originals = {f: f.read_text() for f in {m[1] for m in MUTATIONS}}
    try:
        for name, runner in (("python", run_python), ("rust", run_rust)):
            rc, failed, _ = runner()
            print(f"baseline {name}: exit {rc}, failed {failed}", flush=True)
            assert rc == 0 and not failed, f"{name} baseline must pass"
        killed = 0
        for name, path, old, new in MUTATIONS:
            src = originals[path]
            assert src.count(old) == 1, f"{name}: snippet occurs {src.count(old)} times"
            path.write_text(src.replace(old, new))
            try:
                rc, failed, compile_error = (run_rust if path == RS else run_python)()
            finally:
                path.write_text(src)
            ok = rc != 0
            killed += ok
            why = "compile error" if compile_error else ", ".join(failed)
            where = "rust" if path == RS else "python"
            print(f"{name} ({where}): {'KILLED' if ok else 'SURVIVED'} ({why})", flush=True)
        print(f"{killed}/{len(MUTATIONS)} mutations killed")
        return 0 if killed == len(MUTATIONS) else 1
    finally:
        for f, src in originals.items():
            f.write_text(src)


if __name__ == "__main__":
    sys.exit(main())
