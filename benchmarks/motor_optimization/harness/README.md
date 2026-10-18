# Motor experiment 2: the optimizer harness (issues #1195, #1201)

This directory compares three optimizers on the frozen spaces A and B of
experiment 2 (#1185). The comparison follows the pre-registration and
amendments 1–3, **amendment 4** (adopted with additions 6 and 7, comment
6244746246), **amendment 5** (validation trigger, method C edge cases,
recording a hang; comment 6245006308), **amendment 6** (method C's label
and held-out currents; comment 6245012709) and **amendment 7** (the
geometry domain, the method safeguards, mesh repair, the 120-FE budget,
the starts and the host rule; revision 1, comment 6247097203, with the
adoption corrections, comment 6247152017, which govern). §2–§7 below follow
them. Amendment 8 (methods D and E on L0; comment 6247152174) is not wired
here: its extension points are the method specs (`driver.METHOD_SPECS`,
(optimizer, resolution)) and the session's per-resolution model records
(`session.json` `models`); its start rule (eligible at L0 and L1) is.

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
| `lhs.py`, `make_starts.py`, `starts.json` | The deterministic LHS starts (committed; seeds 1–4, amendment 7 §5). |
| `crates/geode-core/src/mesh/icem_domain.rs` | The geometry domain (amendment 7 §1): the guard inventory, raw margins, scales, `h`, `∂h/∂x`, buildable and admissible. |
| `crates/geode-core/tests/motor_experiment2_domain.rs` | CI target of the domain (no FEM): guard/construction agreement, Jacobian against FD, the pilot designs, θ0 and Opt120, the zero discriminant, the gmsh attempts, the inventory record. |
| `domain_inventory.json` | The guard inventory, every scale and τ floor, and the designs each excludes (written by that target). |
| `recover_pilot_failures.py`, `pilot_failures.json` | The design points of the pilot's 13 failed requests, recovered by re-driving the pilot's own harness on its traces. |
| `test_safeguards.py` | The amendment-7 safeguard tests, the host rule, and #1197 (K2, K4). |
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
    the quality of the moved mesh, and its geometry domain.
  - `geometry` (amendment 7 §1): the domain at `x` (every guard's raw
    margin with its unit, scale, `c`, `h`, `∂h/∂x` over the free
    components, pass/fail; the discrete guards; buildable, admissible,
    `jacobian_defined`). Charged `geometry_check`. No mesh, no solve.
  - `mesh_check` (amendment 7 §3): the training-mesh step alone at `x`
    (move; if the policy fires, remesh with repair). A fresh mesh that
    passes becomes the training reference, as a `train` at `x` would make
    it. The restoration probe of a mesh-infeasible request.
- **Refusals before any mesh or solve.** `train`, `validate` and
  `mesh_check` refuse a design that is not buildable (`refused: not
  buildable`, charged its geometry check only). A gradient `train` where
  the design derivative is undefined (a zero slot-top discriminant) replies
  `status: derivative_undefined` with no value. A `train` or `validate`
  whose every repair attempt fails replies `status: mesh_infeasible` with
  the attempts (each one's outcome, `.msh` SHA-256 and times), charged to
  `remesh`. A failed request's reply carries its `x_bits` and the attempt
  history.
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
- **Budget (amendment 7 §4).** 120 × unit for every method. The 60-FE
  checkpoint (`checkpoint_60` in every trace) counts only validations that
  *completed* by 60 FE; work in flight at 60 FE that finishes later earns
  nothing there. Full curves to 120 FE are kept.
- **Budget stop.** A request still running at the budget is aborted, and the
  evaluator's whole process group (gmsh included) is killed. The exact
  remainder is charged to the request's category, so an exhausted run is
  charged the budget exactly. Nothing after the stop is charged. The run is
  recorded as `stop_reason: budget` with `aborted_request` naming the
  request in flight (its category, op and x), for a gmsh hang as for a slow
  request (amendment 5 §3). `failed` is reserved for error replies.
- **Attainment.** A target counts only from a validation that completed
  within the budget. It is stamped with the cumulative wall-time FE and
  solve-count FE after that validation's charges.
- **Failures.** A failed request ends the run as `failed` (a real solver or
  infrastructure error only), with its design point in `events`. A method
  that stops on its own ends as `converged`; `restoration_failed` is
  amendment 7 §2. All are recorded.
- **Wall time.** `in_run_wall_seconds` (the ledger: the budgeted quantity)
  and `post_run_wall_seconds` (C's diagnostic) are separate (the #1200
  Judge's finding 3).

## 6. Starts (§6; amendment 7 §5 and amendment 8)

- **Generator.** `lhs.py` is a SplitMix64 Latin hypercube, with the same
  arithmetic as `latin_hypercube` in `icem_harness.rs`. Seeds 1–4 each
  draw 10 points in space B's 18-dimensional unit box; space A takes the
  first 8 components (scalars matched across spaces).
- **Rule.** A start is valid when, in both spaces and on both the L0 and
  L1 θ0 meshes, it is admissible (the actual strict guards), buildable,
  and the θ0 mesh *moved* to it passes the policy without a remesh. Seeds 1
  and 2 keep the pilot indices 1 and 4 if valid, else the next valid index;
  seeds 3 and 4 take their first valid index. Every tried index and its
  reasons are in `starts.json`; the preflight is geometry and mesh motion
  only (no FEM), its cost recorded there outside every run budget.
- **Result** (`../logs/make_starts.log`).
  - lhs1 = index 1, lhs2 = index 4: both pilot starts kept (min h 0.547
    and 0.561).
  - lhs3 = index 0 (min h 0.327).
  - lhs4 = index 7 (min h 0.502). Indices 0, 1, 3–6 fail the moved-mesh
    policy at L0 or L1 in some space; index 2 has no slot-top arc.
- **CI.** The CI targets re-derive every start bit for bit and check it at
  L0 (the L1 meshes are gitignored; the L1 half is the log).

## 7. Amendment 7: the domain and the safeguards

- **Domain (§1).** `icem_domain.rs` lists every guard of the two
  constructions (`domain_inventory.json`): 11 smooth guards in space A
  (the magnet, the opening, the slot-top discriminant (m², non-strict),
  the arc centre and radius, the fillet radius and centre, the flank
  length), plus the 55 control-point coupling guards in space B, and one
  discrete guard (`components_finite`). Each smooth margin has the
  construction's own strictness (no epsilon); buildable is "the
  construction succeeds", which a CI test checks agrees with the guards on
  8000 points. Scales are `|m_i(θ0)|`; no θ0 margin is zero, so there are
  no zero-reference scales. A margin behind a failed prerequisite (the
  arcsine of the opening, the square root of the discriminant) is never
  computed: its safe value is `c = −1` with a zero derivative. At a zero
  discriminant the derivatives behind the root are undefined: they are
  `null`, never invented, and a gradient request there is refused. The
  space-B channel wall (moved corner to the opening end) has no
  construction guard; it is recorded as a known meshing risk handled by
  mesh repair and mesh-infeasible handling, not a new constraint.
- **Credit (correction 2, with the #1185 ruling on the tolerance).** A
  target, and the best feasible point, need admissibility on top of the F
  and mean rule: buildable, the discrete guard, a valid validation mesh,
  and every `h_i ≥ −1e-12` (`CREDIT_H_TOLERANCE`, the same for every
  method; it absorbs the rounding of a constraint an optimizer holds
  active, `h ≈ −5e-16`). Both sides decide it (Rust
  `Domain::credit_admissible` and `Acceptance::with_admissibility`; the
  driver from the raw `h`, cross-checked). The optimizers' constraints,
  buildability (the exact construction guards), the restoration anchor
  and the start rule keep `h ≥ 0` exactly. A validation that is
  mesh-infeasible has no value and no credit.
- **Refused points (§2).** Not buildable, mesh-infeasible, or a gradient
  where the derivative is undefined. No FEM value or gradient is made up or
  returned for another point; each refusal is recorded with its x, its
  geometry (margins, scales, h, discrete guards) and its costs.
- **Restoration (§2, correction 1).** From the last *admissible* evaluated
  point toward the refused x: `y_k = x_v + 2^-k (x − x_v)`, k = 1..20; the
  first admissible `y_k` (for a mesh-infeasible x, also one whose
  `mesh_check` passes, its attempts charged) is restored. None:
  `restoration_failed`.
- **A, safeguarded SLSQP with restarts.** SLSQP sees the mean constraint
  and every `h_i ≥ 0` with their gradients. A refused request aborts the
  `minimize` call; restoration; a new `minimize` from the actual `y` with
  fresh state. At most 20 restarts. Each restart records the rejected x,
  the restored y, the restoration's probes and costs.
- **B, COBYQA.** A refused x returns NaN for the objective and the mean
  constraint (the pinned scipy's extreme barrier); the geometry constraints
  are their real values. The initial set is COBYQA's own 2n + 1 points, a
  refused one counting in it. Results come only from validated attainment.
- **C.** Refused points are skipped and never fitted. A refused candidate
  is a rejection in every respect but the fit (the #1185 ruling): Δ is
  halved, the counter advances, and it counts toward the geometry point of
  every second consecutive rejection, which is itself checked for
  buildability (and a refused geometry point is skipped). The initial set tries
  +σ, −σ, then σ/2, σ/4, σ/8 each way (+ first unless it leaves the box); a
  coordinate with no buildable step is `restoration_failed`. The subproblem
  includes `h ≥ 0`. The feasible-first ordering uses the total violation:
  the mean-floor violation plus Σ max(0, −h_i), and a failed discrete
  guard is infeasible.
- **Mesh repair (§3, with the #1185 ruling on attempt 2).** At most 4
  attempts, each from the defaults plus its own options only
  (`icem_remesh::MESH_REPAIR_ATTEMPTS`): the default (`Mesh.Algorithm =
  6`, Frontal-Delaunay); `Mesh.Algorithm = 5` (Delaunay); `Mesh.MeshSizeFactor
  = 0.9`; `= 0.8`. The registered attempt 2 (`Mesh.Optimize = 1` with
  `Mesh.OptimizeNetgen = 1`) acts on tetrahedra only and gave the default's
  2-D mesh byte for byte; the ruling replaced it. At θ0 every attempt now
  gives a different mesh (`../logs/mesh_repair_attempts_L0.log`). Every
  attempt is charged and recorded (outcome, `.msh` SHA-256, times). All 4
  failing is `mesh_infeasible`; `failed` stays for real errors.
- **Limitations (disclosed).** (1) Near the slot-flank boundary the
  geometry itself can have an angle below 5° (a point at 4.017° failed all
  four attempts with the same minimum angle): there the minimum-angle
  policy acts as an implicit domain constraint, which mesh repair cannot
  lift. (2) The space-B channel wall (moved slot-opening corner to the
  opening end) has no construction guard and is 0.16 µm long at SW3 at
  its minimum with CPS1 at its maximum; it is left to the remesh policy,
  mesh repair and mesh-infeasible handling, not made a new constraint.
- **Host (§6).** `session.py`: the pre-run 1-minute load must be at most
  N + 1; a run that cannot start within `--max-wait` is `deferred` (not
  started) and requeued, up to `--max-deferrals`. A run whose in-run phase
  (trace `in_run_start_time`..`in_run_end_time`, not C's diagnostic) has
  load above 1.5 N for more than 60 s is `host_contaminated` and re-run at
  the end of the session; both records are kept. The unit calibration is
  gated and monitored the same way. `host_idle: true` only if nothing was
  contaminated or every contaminated run was re-run cleanly.
- **Motion (disclosure).** The two spaces move the θ0 mesh differently:
  space A with the Jacobian-stiffened harmonic extension (χ = 1), space B
  with the plain one (χ = 0, `CP_MOTION_DEFAULT`). An evaluator process
  serves one space, so every moved-mesh decision (the start rule's `check`,
  the restoration's `mesh_check`, the training remesh trigger, the
  validator) uses its own space's motion; a CI test pins it.
- **#1200 Judge's gaps.** Every training remesh `.msh` SHA-256 is in the
  trace (`remesh_meshes`, per attempt); every failed, refused or aborted
  request records its x; in-run and post-run wall time are separate.

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
- **Python (`test_harness.py`, 31 tests, and `test_safeguards.py`, 38 tests; CI job
  `motor-harness-driver`).**
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

`mutation_checks.py` makes 82 single-snippet mutations, all killed
(`../logs/harness_mutation_checks.log`, one unedited run): 59 of the
driver, session and acceptance (the Python tests), 16 of the evaluator (the
`motor_experiment2_harness` and `motor_experiment2_domain` targets), 5 of
the domain (that target and the `icem_domain` library tests), 1 of the
remesh loop (the `design_remesh` library tests) and 1 of the repair
sequence. 43 bind the amendment-7 rules of #1201 (strictness, τ, the
credit tolerance on both sides (zero, and too wide), the
derivative domain, safe evaluation, the evaluator's refusals and credit,
the driver's credit and best feasible, C's total violation, the start
check, the refusal before any FEM, the restoration anchor, the restart at
y, the bisection and restart limits, the mesh probe, COBYQA's NaN, C's
−σ retry, halvings, refusal-as-rejection and its geometry point, the
non-cumulative attempts, the registered repair, the attempt history, the
charged attempts, the checkpoint, the host gate, factor, limit, interval
and idle rule, the calibration gate, the remesh hashes, the failed,
transport-failed and aborted request points, the wall-time split and the
validation record's evidence); 3 are #1197's K2 (B and C) and K4. A mutant
that loops is killed by the Python suite's timeout. The #1193 script
(`../mutation_checks.py`, 35 mutations of the validation layer) was
regenerated after the Validator's repair loop changed: 35/35 killed
(`../logs/mutation_checks.log`). The four the issue #1195 names:

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

- **Amendments 4–6** (`../smoke/`): θ0, both spaces, the three methods,
  budget 20 × unit, six processes; host loaded.
- **Amendment 7** (`../smoke/amendment7/`, `SUMMARY.md`): both models at
  L0, an L0 anchor, budget 20 × unit (unit 3.358 s, six-way), six
  processes, 21 runs. Host shared (load 17–22 on 28 cores), so the gate
  was **off** (`--host-gate off`, smoke only) and contamination was only
  recorded. The safeguards it exercised:
  - B's NaN barrier at non-buildable (4) and mesh-infeasible (1) points;
    C skipping non-buildable (2) and mesh-infeasible (5) points, its −σ
    initial-set retry (2), and refused steps counted as rejections, two of
    them reaching the every-second-rejection geometry point;
  - A's restoration of mesh-infeasible requests: 7 restorations with
    `mesh_check` and 7 restarts at the restored y, then one
    `restoration_failed` (the injected mesher failure in every attempt);
  - mesh repair succeeding after a failed attempt (5, the `repairdrill`);
    a real, not injected, mesh-infeasible point in the first smoke (all 4
    gmsh attempts at a 4.017° minimum angle: the angle is the geometry's;
    see the limitations);
  - the credit tolerance: 5 validations at h between −1e-16 and −5e-16
    (an optimizer holding `fillet_centre_y` active) are credit-admissible
    though not h ≥ 0; none was further out;
  - the gate drill (deferred twice, never started) and the gated,
    monitored calibration.
  - **Not exercised by the smoke:** A never asked for a non-buildable
    point (its `h` constraints kept it inside; from the boundary start it
    stopped on the boundary), and no design hit a zero discriminant. Both
    paths are bound by the tests (`test_safeguards.py`,
    `motor_experiment2_domain`).
- **Not data.** It is stored apart from `runs/` and shows only that the
  harness and its safeguards run end to end.

## Reproduce

```bash
export CARGO_TARGET_DIR=/path/to/private/target
cargo build -p geode-core --release --example icem_harness_eval
H=benchmarks/motor_optimization/harness
$H/pyrun.sh $H/test_harness.py
$H/pyrun.sh $H/test_safeguards.py
cargo test -p geode-core --release --test motor_experiment2_harness --test motor_experiment2_domain
GEODE_WRITE_DOMAIN_INVENTORY=1 cargo test -p geode-core --release --test motor_experiment2_domain \
    the_domain_inventory_is_the_committed_record
cargo test -p geode-core --release --test motor_experiment2_domain -- --ignored   # gmsh attempts
# The L1/L2 θ0 meshes (gitignored): run_all.sh meshes, or build_sector_mesh.py --levels 1 2
$H/pyrun.sh $H/make_starts.py --binary $CARGO_TARGET_DIR/release/examples/icem_harness_eval
# The pilot failures: the pilot's evaluator (SHA-256 in runs/pilot/manifest.json) and its
# compile-time reference path; --check re-derives the replay-mode ones (CI).
$H/pyrun.sh $H/recover_pilot_failures.py --binary PILOT_EVALUATOR
$H/pyrun.sh $H/session.py --smoke --procs 6 --budget-fe 20 [--host-gate off] \
    --out benchmarks/motor_optimization/smoke/amendment7
$H/pyrun.sh $H/summarize_session.py benchmarks/motor_optimization/smoke/amendment7
$H/pyrun.sh $H/mutation_checks.py
# Main runs (not run here; they need the operator's host):
# $H/pyrun.sh $H/session.py --main --procs N --out benchmarks/motor_optimization/runs/main
```
