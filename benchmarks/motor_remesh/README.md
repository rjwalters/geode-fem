# Motor design loop: quality-triggered remesh-and-transfer

Issue #1184 is a prerequisite of experiment 2 (#1185, pre-registered in
the issue comment of 2026-10-17). The design maps of #1176 (12 scalars) and
#1180 (12 scalars and 10 control-point offsets) move the nodes of one mesh
on a fixed topology. Far from the mesh's own design the moved mesh
degrades: Opt120 inverts 4 triangles at L3, and near the `MAG`, `SW4` and
`SD1` bounds the smallest angle falls to 0.1–0.35° (#1182). This directory
holds the API that lets the optimizer leave that region, and its
validation. No optimization was run.

Every statement below is bounded by the runs in `runs/`. A statement
labelled *Hypothesis* has not been tested.

## Verdict

**Observed.**

- **One remesh event on each path.** The path is
  `θ(t) = θ0 + t (θ_Opt120 − θ0)` over all 22 components, evaluated at
  `t = 0, 0.1, …, 1`. At L1 and at L2 it fires the secondary trigger once,
  at `t = 0.8`.
  - The moved minimum angle there is 4.91° (L1) and 3.57° (L2).
  - The scan of the θ0 mesh first crosses 5° at `t = 0.80` (L1) and
    `t = 0.72` (L2), on a 0.01 grid.
  - Neither level inverts a triangle along the path. The minimum angle
    at Opt120 is 2.18° (L1). The L3 inversion of #1182 was not rerun
    here.
  - After the event the fresh reference carries the path to Opt120 with
    no further event. Its minimum angle stays at 30.6° or more (L1) and
    32.3° or more (L2).
- **Remesh cost.** The fresh mesh (`build_design_mesh.py` through
  `uv`, interpreter start included) takes 1.17 s at L1 and 2.00 s at L2.
  The set-up on it (read, sliding band, attachment and factorization of
  the design map, shape problem) takes 0.07 s and 0.36 s. For scale, one
  evaluation with gradients takes 18–38 s at L1 and 44–94 s at L2
  (3 currents × 20 angles, forward and adjoint). The topology changes
  every time: 9937 → 9298 nodes at L1, 36083 → 35007 at L2. All wall
  times are load-perturbed: three to four single-threaded runs shared a
  28-core host whose load average was 11–26.
- **Continuity across the event.** Moved mesh (old reference, frozen) and
  fresh mesh at the event design, same model:

  | level | F rel | dF/dt rel | gradient ‖Δ‖/‖·‖ | gradient cosine | Δ mean(1) | std(1) rel |
  |---|---|---|---|---|---|---|
  | L1 | 1.68% | 0.38% | 0.85% | 0.999989 | +2.9e-4 N m | 0.98% |
  | L2 | 0.40% | 0.30% | 0.29% | 0.999996 | +7.6e-5 N m | 0.14% |

  The jump falls with refinement. At L2 it is the size of the Opt120
  moved-vs-fresh gap on the same model (0.37% in F). It is below the
  0.55% h45 `F_train` gap that #1182 measured between the moved and
  fresh export meshes of Opt120 at L2. It is far below the
  pre-registered validation margin `δF = 0.10 F`. The θ0 gap, between the
  NURBS-multipatch mesh and a fresh construction mesh of θ0, is 0.34% (L1)
  and 0.038% (L2). The event jumps sit above it because the moved mesh at
  the event is degraded.
- **Gradients next to the event.** Central differences were taken on a
  frozen reference: at `t = 0.78` on the old one and at `t = 0.82` on the
  new one. The FD error relative to the adjoint is as follows.
  - Directional `dF/dt`: 7.9e-6 to 9.5e-6 at `h = 1e-3`, and 5.3e-8 to
    8.5e-8 at `h = 1e-4`, on both sides at both levels. The error falls
    by about 100 for the 10× smaller step, so `h = 1e-3` is
    truncation-dominated.
  - `d mean(1)/dt`: 1.9e-8 to 2.3e-8 at `h = 1e-4`.
  - Components `MAG`, `SW4`, `SD1`, `CPR1` and `CPS1` at `h = 1e-6` m:
    1.8e-9 to 1.9e-5.

  The old reference at `t = 0.78` has a minimum angle of 5.26° at L1
  and 3.87° at L2. Its gradient is still consistent with its own
  objective; the remesh is about the objective's mesh error, not the
  adjoint's consistency.
- **Bit-identity when no trigger fires.** The evaluator reproduces the
  existing path bit for bit, compared with `design_gradient_from` on the
  existing set-ups. Every torque, mean, std and gradient component
  matches, cold and warm-started:
  - at L1 in `runs/identity_L1.json`: 198 values per point at θ0 and at
    `t = 0.3`;
  - at L0 in CI (`tests/motor_design_remesh.rs`): both design spaces,
    plus the warm-started chain.
- **Warm start across the remesh.** The P1 transfer works.
  - On the event's fresh mesh, the transferred start converges to the
    cold solution's `F` to 1.4e-12 (L1) and 1.1e-11 (L2) relative. It
    extrapolates 2 nodes, by at most 4.8e-16 m.
  - It does not pay: it takes 580 and 660 Newton iterations against 264
    and 267 cold.
  - The same holds without a remesh. The previous design's field as
    start takes 300–381 iterations per evaluation, against 257–267 cold
    (`path_L1_cold.json`).
  - At the tolerance used here, the cold start is the cheaper choice and
    is the default (`EvaluatorConfig::training`). Its first Newton step
    is the constrained linear solution.
  - Cold and warm paths give the same `F` to the printed 7 digits at
    every point.
- **Near the optimum the gradient gap is large.** At Opt120, the last
  reference (moved from the event's fresh mesh) and a fresh mesh of
  Opt120 itself agree on F to 0.18% (L1) and 0.37% (L2). Their gradients
  differ by 42% (L1) and 6.7% (L2) in norm (cosine 0.91 and 0.998): the
  gradient there is small, and the mesh-to-mesh discretization difference
  is a large part of it. Along the path direction the difference is
  larger still: `dF/dt` differs by 108% at L1 (−5.0e-6 moved against
  −1.04e-5 fresh, `tables.md`) and by 17% at L2 (−5.9e-6 against
  −6.8e-6), because F is nearly stationary along the path there (`dF/dt`
  is about 1e-4 of its θ0 value).

**Hypotheses (not tested here).**

- An optimizer that remeshes near its optimum will see a gradient jump of
  the size above: 42% of the gradient norm at L1 and 6.7% at L2, and 108%
  (L1) and 17% (L2) in the derivative along the path. Stopping rules based
  on the gradient norm or on a directional derivative would then need a
  floor at that size.
- The primary trigger (inversion) is expected to fire at L3 near Opt120,
  where #1182 recorded 4 inverted triangles. This was not run, because L3
  is outside the pre-registered training and validation models except as
  a spot check.

## The API

The API is Rust, generic over the two design spaces.

**`geode_core::mesh::design_remesh`** (library; solver-free, unit-tested):

- `RemeshPolicy` sets the triggers. Its default is the pre-registered
  rule: inversion first (primary), then a minimum angle below
  `MIN_ANGLE_TRIGGER_DEG = 5°` (secondary). The minimum angle is taken
  over positive-area triangles only, so it is never the inversion test.
- `RemeshingReference<S, R>` moves the reference, checks the moved mesh
  and, on a trigger, remeshes at `θ`. The fresh mesh then becomes the
  reference for every later design.
  - Each event is recorded as a `RemeshEvent`: trigger, moved and fresh
    quality, sizes, topology change, and the mesh, set-up and total wall
    times.
  - `prepare` never returns positions that fire the policy. A failed
    remesh, a fresh mesh that fires the policy, or a design without
    geometry returns an error and leaves the reference unchanged.
  - `check_on(generation, θ)` is the frozen check for FD stencils. It
    refuses an inverted mesh and reports, but allows, a small angle.
  - Retired references stay reachable on request (`with_retired`).
- The traits are `MovingMesh` (implemented for `IcemShapeMap`,
  `IcemCpShapeMap`) and `Remesher`.
- `NodalTransfer` moves nodal fields by P1 interpolation, part by part.

**`tests/common/icem_remesh.rs`** (the evaluator an optimizer calls,
included by `#[path]` like the other motor drivers):

- `RemeshEvaluator::<CpSpace | ScalarSpace>::new(msh, θ0, reference,
  EvaluatorConfig, MeshSource)`.
- `evaluate(&point, gradient) -> DesignEvaluation` returns:
  - per current: the torques, mean, std and their gradients over all
    components (22 or 12);
  - `F = Σ std²` and `dF`;
  - the generation, the remesh event if one fired, the mesh quality,
    the start used and the wall times.
- `evaluate_on(generation, &point, gradient, start)` is the frozen
  evaluation used for FD and moved-vs-fresh comparisons.
- `EvaluatorConfig::training(level)` is the pre-registered training
  model: h45 coupling torque, g20 (`0..20`°), currents 0, 0.5 and 1 of
  9.6777 A, fixed peak current, cold start, default policy.
- `MeshSource::Gmsh { out_dir }` runs
  `../motor_shape_gradient/design/build_design_mesh.py` (gmsh 4.15.2 and
  numpy 2.5.4 through `uv`, `python -I`). `MeshSource::Fixed` is for
  tests.
- The solve and gradient are the existing `icem_design` /
  `icem_cp_design` functions. Two changes were needed: the set-ups gained
  a `degrees` field (default: the old `OPT_DEGREES`, so existing callers
  are unchanged), and `design_gradient_from` takes a warm start.

**Not covered.**

- The Jinit control rule.
- The h93 validation coupling, which has no `Mode` yet.
- A remesh path in the 12-scalar space. The bit-identity in that space
  is covered at L0 in CI; its remesh mechanics are the same generic code.

## Runs

| file | what |
|---|---|
| `runs/path_L1.json`, `runs/path_L2.json` | the path with warm starts. Contents: the 0.01 quality scan, the 11 evaluations with gradients, the event with its moved and fresh evaluations, the FD blocks before and after, the gaps at θ0 and Opt120, and the fresh-mesh logs |
| `runs/path_L1_cold.json` | the same at L1 with cold starts |
| `runs/identity_L1.json` | the bit-identity at L1 |
| `logs/*.log` | console output of each cited run (`run_all.sh` also writes `build_initial_L1-L2.log` and `fixture_L0.log` for its mesh stages; they are not cited and not committed) |
| `tables.md` | every table, generated by `analyze.py` |
| `fixtures/icem_remesh_initial_cp_L0.msh` | fresh construction mesh of θ0 at L0 (CI fixture; 2861 nodes, minimum angle 18.07°), with its plan `fixtures/plan_initial_cp.json` |

The model is h45 coupling torque, g20, currents 0, 0.5 and 1 of 9.6777 A
(fixed peak current, unscaled geometry), and the control-point design
space. The Newton tolerance was tightened to `rel_tol = 1e-12` (the
training default is `1e-8`) so that central differences resolve the
gradient. Each run is single-threaded (`faer::Par::Seq`,
`RAYON_NUM_THREADS = GEODE_NUM_THREADS = 1`).

The path runs were made with the tree of this PR before a refactor of
`icem_remesh.rs` that only changed how arguments are passed. The L1 path
was rerun with the final binary and reproduced every `F` and gradient bit
for bit.

## Reproduce

```bash
export CARGO_TARGET_DIR=/path/to/private/target
benchmarks/motor_remesh/run_all.sh all   # meshes, fixture, identity, paths
python3 -I benchmarks/motor_remesh/analyze.py
cargo test -p geode-core --release --test motor_design_remesh
```

The initial L1/L2 meshes and the fresh meshes are gitignored. `run_all.sh
meshes` regenerates the initial ones from the upstream cache
(`../motor_reference/extract_reference.py --fetch`); the path runs write
fresh meshes to `meshes/`.
