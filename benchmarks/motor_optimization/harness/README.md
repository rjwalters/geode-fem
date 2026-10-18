# Motor experiment 2: the optimizer harness (issue #1195)

This directory compares three optimizers on the frozen spaces A and B of
experiment 2 (#1185). The comparison follows the pre-registration and
amendments 1–3, **amendment 4** (adopted with additions 6 and 7, comment
6244746246), **amendment 5** (validation trigger, method C edge cases,
recording a hang; comment 6245006308) and **amendment 6** (method C's label
and held-out currents; comment 6245012709). §2–§6 below follow them.

Every cost is charged. Every new incumbent is validated on the validation
model and scored against the anchor; no start is ever validated.

**Method C is an empirical RBF surrogate**: cubic RBF interpolants of
training-model outputs, used in a trust region. It is not a reduced
physical or waveform model (amendment 6 §1). A comparison against a
physics-based inexpensive model remains open (amendment 6 §4).

This directory holds the harness and its checks. **It holds no pilot
results.** The pilot (36 runs, #1198; host not idle, feasibility and budget
only) is in `../runs/pilot/`, with its `SUMMARY.md`.

## Files

| file | what |
|---|---|
| `crates/geode-core/examples/icem_harness_eval.rs` | The evaluator process, one per run: JSON lines over stdin/stdout. |
| `crates/geode-core/tests/common/icem_harness.rs` | `Session::handle`: the requests and their charges, built on `RemeshEvaluator`, `Validator::accept`, `Anchor` and `CostLedger`. |
| `crates/geode-core/tests/motor_experiment2_harness.rs` | CI target. |
| `driver.py` | The problem scaling, the charged `Run` (the validation policy and attainment), and methods A, B and C. |
| `ledger.py`, `acceptance.py`, `evaluator.py`, `surrogate.py` | The driver's ledger, its independent copy of the acceptance rule, the process client, and the RBF model. |
| `run.py` | One run: a fresh process, a fresh ledger, one trace. |
| `session.py` | The run manifest: calibrates the unit, then launches every job (`--procs N --out DIR`; the pilot by default, `--smoke` for the smoke). |
| `summarize_session.py` | Writes `SUMMARY.md` for a session directory. |
| `summarize_pilot.py` | The pilot analysis (#1198): attainment, budget profile, feasibility, failures, host load, quality-versus-cost curves (`curves.csv`, `quality_vs_cost.svg`) and method C's held-out-current diagnostic, into the session's `SUMMARY.md`. |
| `lhs.py`, `make_starts.py`, `starts.json` | The deterministic LHS starts (committed). |
| `fd_check.py` | FD check of the gradient replies through the process. |
| `mutation_checks.py` | Mutations of the harness wiring. |
| `test_harness.py` | Driver tests on a fake evaluator with a fake clock. |
| `requirements.in`, `requirements.lock`, `pyrun.sh` | The environment: CPython 3.12, the hash-pinned scipy 1.18.1 and numpy 2.5.4, always run with `python -I`. |

## 1. The evaluator protocol

Every request is a JSON object with `op` and `id`. Every reply echoes `id`
and carries `ok` and `charges`.

- **Requests.**
  - `train`: `x_bits`, `gradient`, `purpose` (`iterate` or `line_search`).
    Replies `F`, the per-current means and stds, `mean_full`, and, with
    `gradient`, `dF` and `dmean_full`.
  - `validate`: `x_bits`. Runs `Validator::accept` on the primary model, at
    the anchor's level and on the anchor's θ0 mesh. Replies the value and
    the `Acceptance`.
  - `calibrate_unit`: measures the unit and the weights (§5).
  - `anchor`: computes an anchor (smoke only).
  - `check`: reports whether `x` is in the box, whether its geometry exists,
    and the quality of the moved mesh.
- **Exact `x`.** `x` travels as IEEE-754 bit patterns (`x_bits`), and every
  reply echoes them. The reason: `serde_json`'s decimal float parse is not
  always correctly rounded, and it moved a committed start by one ulp. For
  the same reason, the driver accepts the evaluator's anchor only within
  4 ulp of the file (the tolerance of `Anchor::from_json`), and then scores
  with the evaluator's exact values.
- **Charges.** `charges` are the entries of a request-local `CostLedger`
  (`charge_evaluation`, `charge_validation`). A failed request charges its
  whole handling time to its category.
- **Refusals.** The evaluator refuses to start on a validation mesh, or at a
  level, that is not the anchor's.

## 2. Method C, the empirical RBF surrogate (amendment 4 §2, amendments 5 §2 and 6)

- **Model.** Cubic RBFs, φ(r) = r³, each with a linear tail, fitted to every
  training evaluation of the run (`surrogate.py`). There are four:
  - the training torque std at current fractions 0, 0.5 and 1, each
    divided by √F_train(θ0);
  - the full-current training mean, divided by mean_train(θ0).

  The surrogate objective is F̂ = Σ ŝ_c². The surrogate constraint is
  m̂ ≥ 1.0025.
- **Calibration.**
  1. Evaluate the start u₀ and the n points u₀ ± Δ₀ e_i, with Δ₀ = 0.1 in
     the unit box (+ unless that leaves the box).
  2. Fit. The evaluations are charged as `forward` and the fit as
     `setup_calibration`.
- **Iteration.**
  1. Solve the trust-region subproblem with SLSQP on the surrogate, within
     the box and ‖u − u_k‖∞ ≤ Δ.
  2. Evaluate the candidate (forward only), then refit.
  3. Accept the candidate if it is better than the incumbent. "Better" is
     feasible-first: a lower violation wins, then a lower F.
  4. On acceptance, expand Δ to min(2Δ, 0.5) when ρ ≥ 0.75 (or the old
     incumbent was infeasible) and the step reaches 0.9 Δ.
  5. On rejection, halve Δ. After every second rejection, add a geometry
     point u_k ± Δ e_j, with j cycling.
  6. If the subproblem returns a point within 10⁻³ Δ of an evaluated
     point, evaluate a geometry point instead. A geometry point that does
     not improve counts as a rejection (step 5; amendment 5 §2 C1).

  The subproblem and the refits are charged as `surrogate_update`.
- **Stop.** At T99, when the budget is spent, or when Δ < 10⁻⁶. On a
  Δ < 10⁻⁶ stop the final incumbent is validated under §4 (amendment 5 §2
  C2). The trust-region steps (Δ, geometry, accepted) are in each trace
  under `surrogate_steps`.
- **New-design checks.** These cost nothing extra. They test new designs
  and a finer model, **not** currents C never saw (0.5 is one of its
  training currents; amendment 6 §2).
  - Before each refit, the new training point is held out: the model's
    predictions of F, the three stds and the mean are recorded against the
    training values (`assessment.held_out_training`).
  - At each validation, the predicted std at fraction 0.5, F and the mean
    are recorded against the validation model's values
    (`assessment.new_design_validation_model_checks`). At an accepted point
    the model interpolates its own training value, so this error is the
    gap between the training and validation models.
- **Held-out currents: the post-run diagnostic (amendment 6 §3).** After a
  C run stops, a fresh evaluator process evaluates the final incumbent at
  current fractions 0.25 and 0.75 on the training and validation models
  (`diagnose` request), against C's prediction there. C models only its
  training currents, so its prediction at an intermediate current is the
  linear interpolation, in the current fraction, of its per-current RBF
  predictions at the two neighbouring training currents (0 and 0.5, or 0.5
  and 1). C's std interpolants are its own; the means at 0 and 0.5, which
  C does not model, are RBFs fitted post-run to the same training records
  (`driver.c_current_predictions`). The record is the trace's
  `post_run_diagnostic`, with its own charges and seconds. It runs after
  the run's ledger is closed, so it cannot change the run's budget, path or
  attainment times; a test checks that the trace is otherwise identical
  with and without it.

## 3. Methods A and B, and the common problem (§1 and §3)

- **Variables.** u ∈ [0, 1]ⁿ, the box-scaled free components.
- **Objective.** f = F_train / F_train(θ0).
- **Constraint.** g = mean_train / mean_train(θ0) − 1.0025 ≥ 0.
- **θ0 training constants.** Measured once per session by
  `calibrate_unit`; not charged.
- **A (SLSQP).** The adjoint gradients; ftol 1e-15. Every evaluation
  requests the gradient. The first new point after each gradient use is an
  iterate (`forward`); later new points before the next gradient use are
  `line_search`.
- **B (COBYQA).** scipy 1.18.1; the default 2n + 1 interpolation points;
  initial trust radius 0.1 (the same as C's Δ₀); final radius 1e-6.

## 4. Validation policy (amendment 5 §1)

Every validation is charged to the method.

- **Never the start.** No method validates its start point.
- **Initial sets.** COBYQA's callback runs after every evaluation (scipy
  1.18.1, cobyqa 1.1.3) with the best point so far; it validates nothing
  during the first 2n + 1 evaluations. C validates nothing during its n + 1
  initial design. When the set is complete, the incumbent is validated once
  if it is not the start.
- **After that.** Each new incumbent: A, the iterate the SLSQP callback
  reports (once per major iteration); B, the callback's best point when it
  changes; C, each trust-region acceptance.
- **Final point.** Validated unless it is the start or already validated.
- **No repeats.** A point validated once in a run is not validated again:
  the value depends only on the candidate.
- Every validation's acceptance is decided twice: by the evaluator (Rust)
  and by the driver (`acceptance.py`, from the reply's F_val and mean). A
  disagreement stops the run.

## 5. Cost, budget, attainment (§5; amendment 3 §1)

- **Unit (amendment 4 addition 6).** Measured under the session's
  concurrency: N simultaneous calibration processes (N = the session's
  process count), 3 cold θ0 validations each; the unit is the median of
  the 3N timings. The single-process median, measured first, is recorded
  as a diagnostic only.
- **Solve-count weights.** One training forward evaluation and one adjoint
  evaluation, each at its median ÷ unit; one validation evaluation = 1.
- **No exclusions.** The driver charges the remaining wall time as
  `overhead`: process start-up, I/O, the optimizer's own work. So the total
  charged equals the run's wall time.
- **Budget stop.** A request still running at 60 × unit is aborted, and the
  evaluator's whole process group (gmsh included) is killed. The exact
  remainder is charged to the request's category, so an exhausted run is
  charged the budget exactly. Nothing after the stop is charged. The run is
  recorded as `stop_reason: budget` with `aborted_request` naming the
  request in flight (its category and op), for a gmsh hang as for a slow
  request (amendment 5 §3). `failed` is reserved for error replies.
- **Attainment.** A target counts only from a validation that completed
  within the budget. It is stamped with the cumulative wall-time FE and
  solve-count FE after that validation's charges.
- **Failures.** A failed request ends the run as `failed`. A method that
  stops on its own ends as `converged`. Both are recorded.

## 6. Starts (§6)

- **Generator.** `lhs.py` is a SplitMix64 Latin hypercube, with the same
  arithmetic as `latin_hypercube` in `icem_harness.rs`.
- **Rule.** Seeds 1 and 2 each draw 10 points in space B's 18-dimensional
  unit box; space A takes the first 8 components. The start is the first
  valid point: geometry exists in both spaces, and the moved L0 θ0 mesh
  passes the remesh policy.
- **Result.**
  - lhs1 = index 1. Index 0's slot flank degenerates.
  - lhs2 = index 4. Indices 0–3 invert the moved mesh.
- **Why the policy condition.** At seed 2's index 0, gmsh fails in space A
  ("Cannot find periodic counterpart") and had not returned in space B after
  10 minutes.
- **CI.** The CI target re-derives both starts bit for bit and checks them.

## Checks

### FD of the gradient replies

Interior point x0 + 0.25 (centre − x0); training model h45, g20; relative
error of the reply against central differences.

| level | space | Newton | dir. dF h = 1e-4 | dir. dmean(1) h = 1e-4 | components dF (h = 1e-6) |
|---|---|---|---|---|---|
| L0, 4 angles (CI) | A | 1e-12 | 2.4e-8 | 5.3e-9 | — |
| L0, 4 angles (CI) | B | 1e-12 | 9.3e-8 | 1.6e-8 | — |
| L1 | A | 1e-12 | 1.9e-8 | 4.7e-9 | MAG 2.9e-7, SW2 5.3e-8, Sr1 2.5e-7 |
| L1 | B | 1e-12 | 9.1e-8 | 4.4e-9 | MAG 1.2e-7, SW2 6.5e-8, CPR1 3.7e-7, CPS1 9.0e-7 |
| L1 | A | default 1e-8 | 9.3e-8 | 6.6e-9 | MAG 7.8e-7, SW2 2.4e-8, Sr1 4.3e-5 |
| L1 | B | default 1e-8 | 3.1e-6 | 8.3e-9 | MAG 3.7e-6, SW2 3.3e-7, CPR1 1.7e-7, CPS1 7.5e-7 |

- **Records.** `../runs/harness_fd_L1_space_{a,b}_{tight,default}.json` and
  `../logs/harness_fd_L1_*.log`.
- **Agreement with #1193.** The L1 rows with Newton 1e-12 equal the #1193
  in-process record at the same interior point
  (`../runs/fd_L1_space_{a,b}.json`).
- **Default tolerance.** With the registered tolerance (1e-8), the
  differences are dominated by the Newton tolerance on small derivatives
  (Sr1: dF = 2.7e-3).
- **Truncation.** From h = 1e-3 to 1e-4, the error falls by 95–155× with
  the tight tolerance.

### Charging and budget

- **Rust (`motor_experiment2_harness`).**
  - A training reply charges mesh motion, forward (or line search) and
    adjoint solves with their counts, plus a remesh when one fired.
  - A validation reply charges its motion and its 60 solves.
  - A refused request charges its handling time.
  - `diagnose` runs both models at the requested currents only, and leaves
    the session's currents (and so the primary validation model) intact.
- **Python (`test_harness.py`, 31 tests, CI job `motor-harness-driver`).**
  - Every request category, plus the driver's `setup_calibration`,
    `surrogate_update` and `overhead`, reaches the ledger in full, and the
    total equals the wall time.
  - Each method, under a short budget, stops at the budget exactly. No
    request goes out after the stop.
  - A validation ending exactly at the budget is credited. One that would
    end beyond it is aborted and not credited, and one whose reply lands
    just past the budget is recorded but not credited.
  - T99 stops the run.
  - No method validates its start, nor anything inside its initial set
    (COBYQA 2n + 1, C n + 1), and no point twice (amendment 5 §1).
  - C halves Δ after a non-improving geometry substitute (C1).
  - The C post-run diagnostic leaves the trace's ledger, attainment,
    validations and events identical (amendment 6 §3).
  - The unit is the median of all 3N concurrent timings (addition 6).
  - An evaluator that echoes other `x_bits`, or replies a flipped
    `attained`, stops the run (the Judge's J3 and J6).

### Mutation checks

`mutation_checks.py` makes 36 single-snippet mutations: 26 of the driver,
10 of the evaluator; all 36 are killed. The four the issue names:

- **Wrong target:** the recorded target, and T90's fraction.
- **δF at the wrong point:** δF at F0, and δF at the training value.
- **Uncharged validation:** in the evaluator, in the driver, and an abort
  charged elsewhere.
- **Validation on the wrong mesh:** the validator built on the training
  mesh, the start-up anchor-mesh check, and the driver's two mesh checks.

The rest bind the budget edge, the credit edge, the abort remainder, the
deadline, the post-stop charge, the T99 stop, the overhead and driver-time
charges, the line-search labels, the unit's median, the full-current index,
the method order, the `x_bits` echo (J3) and `attained` (J6) cross-checks,
the start and initial-set validation rules, C1, the concurrent unit, and
the restoring of the currents after `diagnose`. C2 has no mutation: under
amendment 5 §1 every C incumbent is validated on acceptance, so the final
call can only meet the start or a repeat, which are skipped; removing it is
an equivalent mutant. The log is `../logs/harness_mutation_checks.log`, one
unedited run.

### Smoke run (`../smoke/`, labelled smoke)

- **What.** Both models at L0, an L0 anchor computed for the session,
  θ0 in both spaces, the three methods, budget 20 × unit, six processes,
  under amendments 4–6 (the unit under six-way concurrency).
- **Host.** Loaded: load 9.8–12.5 on 28 cores.
- **Not pilot data.** It is stored apart from `runs/`, and it shows only
  that the harness runs end to end. `../smoke/SUMMARY.md` has the table.

## Reproduce

```bash
export CARGO_TARGET_DIR=/path/to/private/target
cargo build -p geode-core --release --example icem_harness_eval
H=benchmarks/motor_optimization/harness
$H/pyrun.sh $H/test_harness.py
cargo test -p geode-core --release --test motor_experiment2_harness
$H/pyrun.sh $H/make_starts.py --binary $CARGO_TARGET_DIR/release/examples/icem_harness_eval
$H/pyrun.sh $H/fd_check.py --binary ... --space a --level 1 --newton tight --out ...
$H/pyrun.sh $H/session.py --smoke --procs 6 --budget-fe 20 --out benchmarks/motor_optimization/smoke
$H/pyrun.sh $H/summarize_session.py benchmarks/motor_optimization/smoke
$H/pyrun.sh $H/mutation_checks.py
# The pilot (#1198; needs the L1 and L2 θ0 meshes, run_all.sh meshes):
# $H/pyrun.sh $H/session.py --procs 6 --out benchmarks/motor_optimization/runs/pilot
# $H/pyrun.sh $H/summarize_pilot.py benchmarks/motor_optimization/runs/pilot
```
