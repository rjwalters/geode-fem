# Motor experiment 2: prerequisites (anchor, validation mode, spaces, acceptance, cost ledger)

Issue #1193 prepares experiment 2 (#1185). That experiment is
pre-registered in the issue comment of 2026-10-17, with amendments 1 and
2. This directory fixes the numbers every later target depends on: `F0`
and the θ0 validation mean. It also validates the evaluation layer that
the optimizers of #1185 will call. No optimizer was run here.

Every statement below is bounded by the runs in `runs/`. A statement
labelled *Hypothesis* has not been tested. The tables are in `tables.md`,
which `summarize.py` writes.

## The anchor (computed once; `anchor.json`)

The primary validation model is the one of amendments 1 and 2:

- h93 harmonic coupling (n = 3:6:93) with the coupling torque;
- level L2, angle grid g20 (0, 1, …, 19°);
- currents at fractions 0, 0.5 and 1 of 9.6777 A (fixed peak current);
- default Newton tolerance (`rel_tol = 1e-8`), cold start.

θ0 is evaluated on its own NURBS-multipatch mesh:
`geometry/meshes/icem_initial_sector_L2.msh`, 3,515,287 bytes, SHA-256
`00a7eb6f…71cf5`, the same bytes as #1162 and #1182.

| quantity | value |
|---|---|
| **F0** = Σ std² (N² m²) | **9.603404147653e-3** |
| **θ0 full-current mean** (N m) | **1.440839036149** |
| mean floor, ×(1 + 0.25%) (N m) | 1.444441133740 |
| T50 = 0.5 F0 | 4.801702e-3 |
| T90 = 0.1 F0 | 9.603404e-4 |
| T99 = 0.01 F0 | 9.603404e-5 |

- **Provenance.** `anchor.json` records:
  - the commit and whether the `crates/` tree was clean;
  - rustc 1.98.1;
  - `RAYON_NUM_THREADS = GEODE_NUM_THREADS = 1` and faer `Par::Seq`;
  - the host load at start, the mesh SHA-256 and the SHA-256 of
    `reference_data.json`;
  - the per-angle torques.
- **Both spaces agree.** Space A (scalar map) and space B (control-point
  map) give the same θ0 torques bit for bit.
- **The values do not depend on host load.** They are deterministic
  single-threaded results. Only the wall times depend on load.
- **The CI target checks the committed record against the rule.**
  `motor_experiment2::committed_anchor_is_consistent_with_the_rule`
  checks that the targets and floor follow from F0 and the mean, that F0
  is Σ std² of the stored stds, that mean0 is the mean of the stored
  fraction-1 torques (computed in the test, not through
  `full_current_mean`), and the mesh hash, which the anchor itself carries.
- **Agreement with the #1182 record.** On the same mesh, the integer-degree
  subset of `initial_nurbs_L2_h93_fixed` gives F = 9.603404853e-3, which is
  7.3e-8 relative away from F0. The full-current mean matches to 5.6e-9
  relative. The cause is explained in the next section.

## Validation mode reproduces #1182 on the same meshes

`run_all.sh reproduce` evaluates θ0 and Opt120 through the validator,
using the meshes #1182 used. `run_all.sh meshes` rebuilds those meshes,
and `check_meshes.py` matches every SHA-256 against the committed reports
or `MESHES.json`. Each result is then compared, angle by angle, with the
#1182 record.

- **The 0.5° chain reproduces the records to round-off.** The g40 runs
  solve the same 0.5° warm-started angle chain as #1182.
  - θ0, NURBS mesh, L2: the largest per-angle difference is 8.9e-16 N m,
    and F matches bit for bit.
  - Opt120 on the moved θ0 mesh, L2: the largest difference is
    1.2e-13 N m, and F differs by 9e-12 relative.
- **The g20 chain agrees only to the Newton tolerance.** The primary model
  solves g20 as a 1° chain. Each angle starts from the previous integer
  angle, not from the half-degree neighbour as in #1182.
  - The converged torques differ by at most 2.0e-7 N m at every level,
    design and mesh. That covers θ0 NURBS and construction at L2 and L3;
    Opt120 export, construction and moved at L2; Opt120 export and
    construction at L3.
  - F differs by 1e-8 to 8.5e-6 relative.
  - This is the Newton tolerance, not a model difference: on the 0.5°
    chain the same code reproduces the records to round-off.
  - The acceptance criterion "to round-off on the same meshes" therefore
    holds on the records' own chain. On g20 it holds only to the Newton
    tolerance: about 2e-7 N m per angle, and at most 7.1e-10 N² m² in F
    (θ0). That is 350 times below the absolute δF floor of 2.5e-7 N² m².
- **Conforming outer-band check on θ0.** This is Arkkio over tag 5
  (44.5–45.0 mm), through the band-area-guarded path of #1190, on θ0's
  own meshes (NURBS and construction, L2).
  - It reproduces the #1182 `T_arkkio_outer` to at most 2.0e-7 N m per
    angle, the g20-chain level.
  - Its F is 9.796838e-3, which is 2.0% above the h93 F0.
- **Opt120 is refused by the conforming check.** Its stator offsets are
  nonzero (CPS1–3 = 1.3e-8, 5.9e-8, 1.5e-7 m), so
  `ValidationError::ConformingUndefined` is returned
  (`runs/opt120_conforming_refused.json`). This follows amendment 1.
- **The policy path.** Opt120 starts from the θ0 L2 mesh. The moved mesh
  has a 1.33° minimum angle, which fires the secondary trigger. The
  validator remeshes Opt120 with `build_design_mesh.py`.
  - The fresh mesh is byte-identical to the #1182 construction mesh
    (SHA-256 `d847a836…`).
  - Its F is bit-identical to the explicit construction-mesh run:
    3.798504331e-6 N m².
  - The fresh mesh took 2.0 s.
- **L3 spot checks.** θ0 at L3 gives F = 9.481075e-3 (NURBS) and
  9.480544e-3 (construction). Both are 1.3% below the L2 F0.

Opt120 (`ScaleR ≈ 1.048`) lies outside both frozen spaces (amendment 2).
It appears here only as a reproduction case.

## Frozen design spaces

`tests/common/icem_experiment2.rs` defines the two spaces:

- `space_a`: 8 free scalars `MAG MH MW SW1 SW2 SW3 SW4 Sr1` on the scalar
  map;
- `space_b`: the same 8 scalars plus `CPR1..CPR7 CPS1..CPS3` on the
  control-point map.

Properties of the spaces:

- **Bounds.** The scalar bounds come from `reference_data.json`; the
  offset bounds are `OFFSET_BOUNDS`.
- **Frozen values.** `LENGTH = 0.1`, `SD1 = 0.135`, `ScaleR = 1` and
  `OPERATING_ANGLE = 0` are always copied from θ0.
  - The constructors assert these four values against the reference.
  - `coordinates` refuses a full point with any of them changed by a
    single bit, so no free vector can change them.
- **Gradient.** `FrozenSpace::evaluate` returns `dF` and the per-current
  `d mean` restricted to the free components.

**Finite differences.** These use the training model (h45 coupling
torque, L1, g20, Newton 1e-12) on a frozen reference. They are taken at θ0
and at the interior point `x0 + 0.25 (centre − x0)`, along a fixed
direction scaled by the box widths. The adjoint is compared with central
differences (`runs/fd_L1_space_{a,b}.json`):

| space | point | dF/dt rel, h = 1e-4 | dmean(1)/dt rel, h = 1e-4 | components (h = 1e-6 m) rel |
|---|---|---|---|---|
| A | θ0 | 1.4e-8 | 4.1e-9 | MAG 2.8e-6, SW2 6.9e-8, Sr1 3.1e-7 |
| A | interior | 1.9e-8 | 4.7e-9 | MAG 2.9e-7, SW2 5.3e-8, Sr1 2.5e-7 |
| B | θ0 | 2.8e-7 | 2.4e-9 | MAG 2.8e-6, SW2 7.0e-8, CPR1 5.5e-7, CPS1 1.0e-6 |
| B | interior | 9.1e-8 | 4.4e-9 | MAG 1.2e-7, SW2 6.5e-8, CPR1 3.7e-7, CPS1 9.0e-7 |

- **The error behaves as truncation.** The directional error falls by 70
  to 160 times from h = 1e-3 to 1e-4.
- **Stencils at the box edge.** At θ0, space B's offsets sit on their box
  bounds. Its central-difference stencils step outside the box, which
  `embed` allows and `point` refuses.
- **CI repeats the check at L0.** `motor_experiment2` runs the same check
  at L0 with 4 angles. Both spaces and both points must come in below
  1e-5.

## Acceptance evaluator

`accept(anchor, F_val, mean_full)` implements amendment 2, §2:

- δF = max(0.10 F_val, 2.5e-7);
- target T is attained when F_val + δF ≤ T **and**
  mean_full ≥ 1.0025 · mean0;
- the result returns F_val, δF, F_val + δF, the mean, the floor, `mean_ok`
  and, per target, `F_ok`, `attained` and the slack.

`Validator::accept` guards how the rule may be called:

- it runs the validation;
- it refuses a configuration that is not the primary model (mode, g20,
  currents, peak current, Newton `rel_tol` and iteration cap, policy) and
  an anchor of another level;
- it refuses an anchor whose θ0 mesh SHA-256 is not the validator's
  (`ValidationError::AnchorMesh`), so an L2 validator on another θ0 mesh
  cannot be scored against this anchor;
- the validator itself refuses a mesh whose coupling circle does not match
  the level;
- `Anchor::from_validation` takes the anchor only from θ0 on its own mesh.

CI runs this path end to end on the L0 mesh with the primary model
(`anchor_and_accept_use_the_full_current_mean_on_the_anchor_mesh`). The
anchor's mean and the mean the rule sees for a candidate are checked
against fraction-1 means computed in the test, and against the
fraction-0.5 mean, which must differ. A triggered validation is checked to
be bit-identical to a direct solve on the fresh set-up, and different from
a solve on the moved mesh the policy rejected
(`triggering_candidate_is_validated_on_a_fresh_mesh`).

CI tests the rule at its edges, using the adjacent floats `next_up` and
`next_down`:

- each target, in the 10% regime and in the absolute regime;
- the δF switch point;
- the mean floor, which is inclusive. θ0 itself attains nothing, because
  its mean is below the floor;
- NaN inputs, which attain nothing.

**Mutation checks** (`mutation_checks.py`, `logs/mutation_checks.log`).
There are 35 single-line mutations: the 26 of the first round, the six of
the #1194 review (J1 δF at F0, J2 half-current mean, J3 fresh-path solve
on the moved mesh, J4 strict budget edge, J6 validation charged with total
time, J7 Newton tolerance ignored), and three on the anchor mesh (check
removed, anchor from a moved mesh, fresh SHA replaced by the base SHA).
They cover:

- the margin, the targets, the comparison operators and the mean floor;
- the anchor record check, the frozen check, the box check and the
  gradient restriction;
- the conforming fresh-only rule and the stator-offset guard;
- the policy, the fresh-mesh check, the level check and the primary-model
  check;
- the ledger categories, the budget edge and the angle-grid check;
- the full-current mean, the fresh-mesh solve and the anchor mesh.

The log lists which tests killed each mutation.

## Validation-mode rules (decisions made here)

- **History-free.** A candidate's validation value depends only on the
  candidate:
  - the θ0 mesh of the level is moved to it;
  - if the policy fires (any inverted triangle, or a minimum angle below
    5°), a fresh mesh of the candidate is used instead;
  - that fresh mesh must itself pass the policy;
  - every solve starts cold;
  - a fresh mesh is never adopted as a later reference, unlike the
    training loop's `RemeshEvaluator`.
- **Conforming spot check.** It is only computed on a fresh mesh: θ0's own
  mesh at θ0, or a fresh mesh of the candidate. It is never computed on a
  moved mesh, and only for designs with all stator offsets exactly zero.
  The #1190 guard still refuses a band that left its annulus.
- **g40 needs half-degree angles.** The validator solves real angles, which
  must be multiples of the coupling step. This is the same arithmetic as
  `icem_shape::operating_points`. CI checks that the validator equals
  `RemeshEvaluator::evaluate_on` under `EvaluatorConfig::validation`, bit
  for bit.

## Cost ledger (API for #1185)

`CostLedger` lives in `tests/common/icem_experiment2.rs`.

- **Categories.** `setup_calibration`, `forward`, `adjoint`, `line_search`,
  `mesh_motion`, `remesh`, `validation`, `surrogate_update`.
- **Entries.** Each entry has wall seconds, forward equivalents (FE), and
  forward and adjoint solve counts.
- **Helpers.**
  - `charge_evaluation(ev, Purpose::Iterate | Purpose::LineSearch, note)`
    splits a training evaluation into motion, remesh (gmsh plus set-up),
    forward solves (to `forward` or `line_search`) and adjoint solves.
  - `charge_validation(v, note)` charges motion, any fresh mesh, and the
    validation solves.
  - `time(category, note, f)` charges a closure's wall time, for example
    calibration or surrogate updates.
  - `charge(...)` takes an explicit FE value.
- **Totals and budget.** `total(category)`, `total_fe`, `remaining_fe`,
  `exhausted` (budget `BUDGET_FE = 60`), and `to_json`.

**Open protocol point (not decided here).** The pre-registration fixes the
budget in validation-model-equivalent forward evaluations but not how to
convert a wall time, or a training solve, to that unit. `FeRule::WallTime
{ unit_seconds }` is provided. A candidate unit is the θ0 primary
validation solve time: 35.1 s in both spaces in the committed anchor run
(41.4 s and 35.5 s in an earlier run of the same code), measured on a
host with load 10–14 on 28 cores. The ledger keeps raw seconds and
solve counts, so any rule #1185 freezes can be applied afterwards.

## Reproduce

```bash
export CARGO_TARGET_DIR=/path/to/private/target
uv run -q --with scipy==1.18.1 python -I benchmarks/motor_reference/extract_reference.py --fetch
benchmarks/motor_optimization/run_all.sh all   # meshes, anchor, reproduce, fd
python3 -I benchmarks/motor_optimization/summarize.py
python3 -I benchmarks/motor_optimization/mutation_checks.py
```

`runs/*.meshes/*.msh` (the fresh mesh from the policy path) is gitignored;
its SHA-256 is in the run record. Wall times are load-perturbed. Eight
single-threaded runs shared a 28-core host whose load average was 10–18
(recorded per run). Every record in `runs/` and `anchor.json` comes from
one `run_all.sh all` at commit `043b9c47` with a clean `crates/` tree. Two
earlier runs of the same numerical code, at `93a1bbd3` and `f0549cea`, gave
the same values to the bit. `logs/mutation_checks.log` is the unedited
output of `mutation_checks.py` at the same code.
