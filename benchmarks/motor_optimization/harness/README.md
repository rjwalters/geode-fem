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
| `recover_pilot_failures.py`, `pilot_failures.json`, `pilot_replay_host.json` | The design points of the pilot's 13 failed requests, recovered by re-driving the pilot's own harness on its traces. COBYQA's and C's replays are bit for bit only on the pilot's linear-algebra stack (BLAS/LAPACK: Accelerate on macOS, OpenBLAS on Linux diverges after a few steps); the CI test checks the platform-independent part everywhere (construction verdicts, error strings) and the replay only where the stack matches (`pilot_replay_host.json`). |
| `linalg_info.py` | The interpreter's platform, macOS version, numpy/scipy versions and BLAS/LAPACK names and versions, recorded in every session and run record; `same_stack` (every one of those keys) gates the bit-for-bit pilot replay. |
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
- **Platform dependence.** The optimizers' paths depend on BLAS/LAPACK at
  the bit level, and the domain's scales on libm at the last bits: every
  session and run record carries its linear-algebra stack (`linalg_info`)
  and platform; the inventory is compared to 1e-12 relative across
  platforms; a main run is exactly reproducible on its own host and stack.
- **Mesher failures.** `build_design_mesh.py` exits 3 with `MESH_FAILURE` on
  gmsh's own meshing failure; only that is a design-dependent attempt
  failure. Any other non-zero exit (an import or environment error, `uv`,
  a `FAIL:` check, a crash) is an infrastructure error (`failed`, with its
  output).
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
- **Python (`test_harness.py`, 31 tests, and `test_safeguards.py`, 45 tests; CI job
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

`mutation_checks.py` makes 98 single-snippet mutations, all killed
(`../logs/harness_mutation_checks.log`, one unedited run): 71 of the
driver, session and acceptance (the Python tests), 16 of the evaluator (the
`motor_experiment2_harness` and `motor_experiment2_domain` targets), 5 of
the domain (that target and the `icem_domain` library tests), 1 of the
remesh loop (the `design_remesh` library tests) and 5 of the remesher
(the repair sequence, the mesher's exit classification, and the exit code
and marker on either side of the script/Rust contract). 43 bind the amendment-7 rules of #1201 (strictness, τ, the
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
validation record's evidence); 3 are #1197's K2 (B and C) and K4; 16 are
the #1204 Judge's J1–J8, the BLAS gate (G1–G3 and the macOS version), the
exit classification and the exit-code contract (R3 both ways). A mutant
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

### Amendment-8 integration in progress (#1202)

`main_schedule.py` defines the prospective 100-job queue from an explicit
A–E method registry: rotate ABCDE by start index for repeat 1, then reverse
that rotation for repeat 2. It launches nothing. `resource_usage.py` supplies
process-local self/reaped-child CPU deltas and separate RSS high-water
observations. Its child totals remain explicitly incomplete unless the caller
establishes that all owned descendants were reaped. Request CPU must not be
added again to inclusive run totals, and RSS observations are not memory deltas
or a sum of simultaneous process peaks.

Squad review 26 permits private integration against `8bbc516d`; PR and main
execution still require #1201's reviewed merge, rebasing and affected checks.
The resource adapter is now connected to evaluator startup/requests, optimizer
runs, post-run diagnostics and calibration. The five-method registry, queue
connection, recovery and remaining main-study requirements are still pending.
Do not launch the main study from this intermediate implementation.
The queue/resource helper checks are:

```sh
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/test_main_preparation.py
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/main_preparation_mutations.py
```

Twelve tests cover queue completeness/order, CPU attribution across child
lifetimes, RSS units and incomplete accounting; thirteen semantic mutations must
be detected. `main-preparation-checks.json` retains raw output and exact source
hashes. These are synthetic/process-accounting fixtures, not motor outcomes.

`resource_usage.reap_usage` supplies a child-specific `wait4` receipt
for the driver cross-check. Its caller must own the PID and be its sole reaper:
capture this receipt before `Popen.poll`, `wait` or `communicate` can consume
the child's status, retain it, and synchronize the process handle's return
code. Nonblocking observations stay pending while the child lives; already
reaped children and unsupported systems explicitly report unavailable.
Signal termination retains observed usage but never claims complete
descendant accounting. The helper does not drain pipes or kill processes.
Tests use two concurrent children reaped in reverse order to check attribution.
See the [Python wait4 contract](https://docs.python.org/3.12/library/os.html#os.wait4).

The Rust counterpart is
`crates/geode-core/tests/common/motor_resources.rs`. It uses the already-locked
`nix = 0.29.0` safe wrapper as a Unix-only dev-dependency with only its
`resource` feature. It is used by benchmark evaluator code only, not the core library. Other
platforms explicitly report unavailable counters. macOS and Linux RSS units
are normalized separately; other platforms retain an unknown unit.

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test -p geode-core --test motor_resource_accounting
CARGO_TARGET_DIR="$PWD/target" cargo clippy -p geode-core --test motor_resource_accounting -- -D warnings
```

Four checks on the macOS preparation host cover unit conversion, CPU deltas,
separate lifetime RSS observations, incomplete/invalid intervals, and real
self/child CPU. The conditional non-Unix unavailable-path test requires a
non-Unix host and has not been executed here. Driver-side
`wait4` CPU totals cross-check the evaluator's request observations;
startup and between-request work can make the inclusive total larger than
their sum. Do not add request observations again to inclusive run totals.

#### Evaluator lifecycle and resource wiring

The Python evaluator client is its child's sole reaper. It retains a `wait4`
receipt before any ordinary `Popen` wait, closes pipes/logs on every startup
failure, and attaches failure evidence to the exception for `run_one` to save.
After an acknowledged shutdown it allows up to 0.25 seconds for normal exit;
otherwise it kills the owned process group and reports incomplete descendant
accounting. Repeated cleanup retains the original receipt. Request interruption
retains an explicit unavailable request measurement.

`resources` on a run covers its measured Python phase through evaluator cleanup;
Python import and final trace-serialization costs are outside that interval.
The wall-time budget and original attainment records are unchanged. Diagnostics
have their own resource interval outside the stopped run. Calibration retains
each evaluator's receipts and one whole-batch parent/self-plus-reaped-children
interval; concurrent child costs are never attributed using overlapping parent
deltas. Request measurements, child lifetime receipts and inclusive phase CPU
are overlapping views, not summands. Session launcher lifetime receipts and
complete setup/recovery accounting remain pending with the scheduler work.

```sh
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/test_evaluator_resources.py
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/resource_lifecycle_mutations.py
CARGO_TARGET_DIR="$PWD/target" cargo test -p geode-core --release --test motor_experiment2_harness request_resources_cover_success_and_failure_without_summing_rss
CARGO_TARGET_DIR="$PWD/target" cargo build -p geode-core --release --example icem_harness_eval
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/resource_wiring_smoke.py \
  --binary "$PWD/target/release/examples/icem_harness_eval" --out /path/to/new-resource-smoke.json
```

Seven lifecycle/calibration tests cover actual child exits, malformed/EOF/timeout
startup failures, launch failure, interrupted requests, run/diagnostic failure
recording and distinct calibration receipts. Three deliberate lifecycle
regressions must fail with assertions; raw output and source hashes are in
`resource-lifecycle-checks.json`. The labelled `smoke/resource_wiring_1202.json`
checks a compiled evaluator's startup, nonlinear L0 training, expected validation
failure without an anchor, normal shutdown and startup failure. Its child CPU
receipt covers the sum of request measurements. It is all-L0 with four training
angles; it does not establish L0/L2 separation, optimizer performance or main-run
readiness.

### Existing harness commands

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
# compile-time reference path; --check re-derives the replay-mode ones bit for bit only on
# the pilot's linear-algebra stack (macOS arm64, Accelerate; pilot_replay_host.json).
$H/pyrun.sh $H/recover_pilot_failures.py --binary PILOT_EVALUATOR
$H/pyrun.sh $H/session.py --smoke --procs 6 --budget-fe 20 [--host-gate off] \
    --out benchmarks/motor_optimization/smoke/amendment7
$H/pyrun.sh $H/summarize_session.py benchmarks/motor_optimization/smoke/amendment7
$H/pyrun.sh $H/mutation_checks.py
# Main runs (not run here; they need the operator's host):
# $H/pyrun.sh $H/session.py --main --procs N --out benchmarks/motor_optimization/runs/main
```

### Amendment-8 integration in progress (#1202)

The registry now includes D (`cobyqa_l0`) and E (`slsqp_l0`), reusing B/A's
optimizer implementations. They require an explicit L0 model record; a legacy
single-model session cannot silently supply L1. Main scheduling uses the
100-job rotation/reversal queue in `main_schedule.py`; historical pilot and
all-L0 safeguard-smoke ordering remains ABC/CBA. Main setup calibrates L0
training constants/weights separately, reusing the primary calibration's cold
validation unit. Its own validation timing median is retained as a diagnostic.
Every trace retains the selected training-model record.

D/E diagnostics now retain raw nonlinear waveforms at .25/.75, common-validator
provenance and separate costs. Selection uses the lowest-F credited common-feasible
validation (earliest tie), otherwise the last successfully trained optimizer
incumbent, explicitly without a feasibility claim. Paired training values are
retained on failed acceptance, mesh-infeasible validation and interrupted requests.
Waveform arrays are checked against their means/stds. Training mesh hashes identify
the reference before design motion and must be interpreted with the selected point.

This is partial implementation, not launch readiness. Durable
session/recovery, contaminated-calibration refusal and full provenance/cap
reporting remain required. Frozen-start and L0 gradient evidence is described below. The retained scalar-theta0
mixed L0/L2 smoke verifies model separation, not an optimization outcome. Do not
launch a main study from this intermediate branch. `test_baseline_wiring.py`
checks model/validator separation, calibration denominator, shared optimizer
selection, E restart serialization and the actual five-method registry's queue.

The bounded mixed-model smoke runs theta0 L0/h45/g20 training (with adjoints),
registered L2/h93/g20 validation, and a fresh-process .25/.75 diagnostic. It
verifies exact input mesh hashes against the registered anchor, retains raw
replies and resource receipts, and refuses to overwrite its output. L2 is
regenerated from the pinned geometry or copied only after its hash matches the
anchor. Example (after building the evaluator in this worktree's target):

```sh
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/mixed_model_smoke.py --binary target/release/examples/icem_harness_eval --out benchmarks/motor_optimization/smoke/mixed_l0_l2_new.json --space a
```

`test_physical_diagnostic.py` covers selection, failed acceptance, interrupted
validation, unavailable/failed diagnostics and unchanged stopped-run results.
`physical_diagnostic_mutations.py` checks six deliberate regressions in private
copies. These checks and the smoke do not establish optimization performance.

### L0 gradient and frozen-start checks (#1202)

`l0_gradient_check.py` records directional central differences on the frozen L0
reference, with h45/g20 and all three currents. The points and direction are fixed
before solving: theta0 and x0 + 0.25 (centre - x0), the existing box-scaled direction,
and steps 1e-3, 1e-4, 1e-5. The reported criterion uses h=1e-4 and the existing
CI relative-error threshold 1e-5 for both F and full-current mean. Every stencil
must be admissible and pass the moved-mesh policy, and every solve must remain
on generation0 without remeshing. Default and tight Newton records are separate.

| Space / point | Newton | Relative dF error at h=1e-4 | Relative full-current mean error |
|---|---|---|---|
| A / theta0 | default | 2.45e-8 | 8.33e-9 |
| A / interior | default | 7.74e-8 | 1.14e-8 |
| B / interior | default | 1.77e-7 | 2.22e-8 |
| A / theta0 | tight | 1.67e-8 | 4.77e-9 |
| A / interior | tight | 1.98e-8 | 5.46e-9 |
| B / interior | tight | 2.09e-7 | 1.42e-8 |

The planned B/theta0 central stencil is **inapplicable**: theta0 is on the CPR1
upper bound0, and the +h perturbation crosses that bound. Both raw records retain
that pre-solve rejection and the script exits nonzero; neither is presented as an
all-points pass. No point or step was selected after seeing gradient outcomes.
All replies, failures, source/binary hashes and child resource receipts are in
`../smoke/l0_gradient_{a,b}_{default,tight}_1202.json`. These are setup checks, not
optimization timing or a speedup result.

`start_preflight.py` rechecks the committed five starts on L0 and L1 in both
spaces, using `check` only (no FEM or remesh). It preserves raw replies and costs,
refuses output reuse, never changes start identities and rejects wrong design or
mesh identities, domain/quality failures and unexpected solve counts. All20 checks
passed in `../smoke/start_preflight_1202_final.json`, with zero reported FEM solves.
The main launcher runs this gate before calibration or optimizer dispatch; the
test suite verifies that a failure prevents both. `setup_validation_mutations.py`
detects four deliberate point/quality/gate/FD-step regressions.

An additive **boundary start, one-sided** check covers B/theta0, where all
control-point offsets lie on bounds. Its plan was recorded before execution in
[#1202 comment 6248960725](https://github.com/rjwalters/geode-fem/issues/1202#issuecomment-6248960725),
following Claude's review. `l0_gradient_check.py --boundary-start --space b`
flips only the bound-active offset signs inward, keeps the scalar components,
and uses (-3 f(0) + 4 f(h) - f(2h))/(2h) at the same three steps. Both default
and tight Newton cases pass the fixed h=1e-4 criterion:

| Newton | Relative dF | Relative full-current mean | Observed order F / mean, 1e-3 to 1e-4 |
|---|---|---|---|
| default | 1.84e-7 | 2.69e-9 | 2.10 / 2.97 |
| tight | 1.66e-7 | 1.62e-8 | 2.15 / 2.19 |

The smaller-step orders decline (the default mean error increases), consistent
with a numerical/cancellation floor; no best-step selection is made. Raw values,
orders and process receipts are retained in
`../smoke/l0_gradient_b_boundary_{default,tight}_1202.json`. The central-stencil
rejections and all earlier central records are unchanged from commit653f029f.
The original setup runtime receipt binds that earlier source; the additive
`boundary-gradient-runtime-checks.json` binds the new boundary-check source.

### Durable session preparation and inventory (#1202)

The launcher refuses an existing output directory. `events.jsonl` is appended and
fsynced as setup, load samples and attempts happen; each event has a sequence and
timestamps. An advisory OS lock identifies the live session owner. Session/queue
and calibration artifacts are created once and bound by hashes before dispatch.
A completed but contaminated or inconsistent main calibration is archived and
rejected before optimizer dispatch. Per-process calibration replies and resource
receipts are saved even when another calibration process fails.

Attempt creation precedes its idle gate, dispatch precedes process launch, and the
terminal event binds the saved trace hash. Existing trace/log paths are refused.
Worker exceptions remain in scheduler results. A torn last journal line is retained
as an incomplete tail, never interpreted as a finished attempt. Monitoring errors
cannot establish uncontaminated work. Atomic writes protect owned JSON snapshots.

A read-only inventory can be exported with:

```sh
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/recovery_inventory.py --source SESSION_DIR --out NEW_INVENTORY.json
```

It verifies bound metadata and completed traces, preserves the earliest eligible
completion (including ordinary scientific failure/censoring), and lists never-started,
deferred, contaminated and interrupted work. It does not resume or relaunch anything.
`recovery-policy-proposal.md` records the adopted disposition for external interruption
(Squad review 28; #1185 comment 6249073475);
its implementation still needs independent review before main execution. Owned-process cleanup,
launcher lifetime receipts, orphan verification and linked recovery execution remain
required. The current code is not a completed recovery workflow or main-run approval.

### Stopped optimizer evidence and cooperative signals (#1202)

`run.py` now writes `NAME.optimizer-stopped.json` before post-run computation,
then a hash-linked `NAME.json` with `diagnostic_status: pending`. The stopped
artifact is immutable. The final trace is replaced atomically, and existing run
outputs are refused. NumPy scalar booleans are serialized as JSON booleans.

The stopped artifact contains the completed optimizer result, including the
ledger, validation records, target attainment and 60-unit checkpoint. It also
freezes C's current-interpolation input points/incumbent or D/E's selected design.
`reconstruct_diagnostic_run` rebuilds only those diagnostic inputs, with no
optimizer or ledger continuation. The tests reproduce all three methods'
diagnostic rows from the frozen inputs without invoking optimization.

The isolated run CLI handles SIGINT/SIGTERM. Computation can be interrupted;
evidence writes and cleanup retain a pending signal without tearing the snapshot.
An interrupted optimizer preserves completed validations and measured partial
costs, marks `optimizer_completed: false`, and does not start a diagnostic. A
post-run interruption preserves the completed optimizer and marks the diagnostic
unfinished. The runner kills its busy evaluator's process group and reaps it;
killed-child resource accounting remains explicitly incomplete. Observing a
signal does **not** by itself verify an external cause or authorize a replacement.
Scientific errors remain distinct terminal outcomes and retain their earlier
validations too. The original interrupted/failed records are not overwritten.

The read-only recovery inventory verifies the stopped snapshot hash and its
agreement with the trace before classifying a clean completed optimizer with an
unfinished diagnostic as `completed_optimizer_diagnostic_pending`. It is never
an optimizer-launch candidate. Unknown sessions or incomplete journal transitions
still remain unresolved; this does not infer completion from an unbound file.

`test_run_interruption.py` includes real subprocess SIGTERM/SIGINT cases during
startup, optimization and diagnostics, checking evaluator/child termination and
retained receipts. `run_interruption_mutations.py` checks five deliberate
regressions in persistence, interruption, selection and inventory. This is setup
verification, not main-run or FEM-performance data. Session-level cancellation,
per-job launcher lifetime receipts, orphan identity verification, and linked
recovery/diagnostic-only execution remain to be implemented and reviewed.
