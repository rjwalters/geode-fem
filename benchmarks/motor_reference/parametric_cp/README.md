# ICEM free-form control-point design space (M4 prep)

Issue #1179. The reference optimizer's design vector has three parts
(#1176): the 12 scalars, 7 rotor control-point groups (18 points) and 3
stator groups (37 points). GEODE could differentiate the 12 scalars (#1178).
This directory adds the 10 control-point offsets as extra components of `θ`,
in the Rust module
[`geode_core::mesh::icem_controlpoints`](../../../crates/geode-core/src/mesh/icem_controlpoints.rs).
It recovers the iteration-120 offsets, reruns the full iteration-120 design
on a moved mesh, and validates the offset gradients. There is no
optimization here, and no paper text.

**Verdict (measured; every statement is bounded by the runs below).**

- **Groups.** The 18 rotor points are the control points of the rotor
  surface. The 37 stator points are all control points of the bore. The
  groups are tied as listed under [Parametrization](#parametrization). Each
  point moves along its polar direction in the set-up geometry, and that
  direction stays frozen.
- **Fit.** At the iteration-120 scalars, one offset per group fits the
  iteration-120 export's control points with a residual of at most
  6.9e-16 m (all 55 points). All 10 offsets lie inside their boxes:
  - `CPR3` is 2.8e-8 m inside its −1 mm bound;
  - `CPR2` is 1.7e-7 m inside its −1.5 mm bound;
  - the three stator offsets are 1.3e-8 to 1.5e-7 m above their 0 bound.

  With these offsets the construction reproduces every material interface
  of both exports to at most 7.0e-16 m in both directions. That includes
  the rotor surface, which the 9 scalars missed by 1.4 mm (#1176).
- **Moved-mesh forward.** On the initial mesh moved to the full
  iteration-120 design, the full-current mean torque is 1.49857 N m at L2.
  The fresh full mesh of #1176 gives 1.49873 N m, and the saved value is
  1.50001 N m (−0.10%). The largest per-angle difference from the fresh mesh
  is 7.2e-4, 5.0e-4 and 2.4e-4 N m at L0, L1 and L2, falling with h. Without
  the offsets the moved mesh was 11% low. At L3 the fixed topology inverts
  4 triangles at the flux-barrier pockets' outer corners, so L3 was not
  run. That level needs remeshing.
- **Gradients.** Validated for all 10 groups at θ0 and at iteration 120, at
  L0 to L2, with the harmonic coupling and the Jinit current rule. Across
  these runs:
  - the best central difference that re-moves the mesh agrees with the
    adjoint to between 1.8e-11 and 3.0e-8 relative for the mean torque.
    For the std it agrees to between 4e-12 and 1.4e-6;
  - the O(h²) Taylor window over any anchor is 3.5 to 6.0 decades;
  - the largest errors and narrowest windows are on the std at
    iteration 120, which is near a minimum (3e-4 N m).

  At L0 the 9 geometric scalars and ScaleR were also checked in the new
  map: best FD errors of at most 5.1e-8 (mean) and 2.3e-6 (std, at
  iteration 120).
- **Moved mesh vs remeshing.** For `CPR4` and `CPS1` at θ0, the
  least-squares remeshed-minus-moved derivative gap falls with refinement.
  At full current, mean torque, L0 → L1 → L2:
  - `CPR4`: −4.4e-2, −7.1e-3, −1.6e-3;
  - `CPS1`: −2.4e-2, −2.7e-3, −5.1e-4.
- **Mesh quality.** One group at a time, the fixed topology covers each
  group's whole box at L0 and L1. The smallest angle is 11.4° (`CPR3`, L1).
  Remeshing is needed in these cases:
  - **All rotor groups at the far end at once.** With the initial scalars
    this inverts 4 triangles at L1, at the pocket corners.
  - **All stator groups at +0.5 mm with the iteration-120 scalars.** This
    inverts 210 triangles at L0 and 796 at L1. There the 0.5 mm offset
    equals the opening depth SW3 = 0.50 mm, so the opening channel
    collapses. That part of the joint box is degenerate geometry; the
    constraint functions of the reference were not evaluated here.
  - **The full iteration-120 design at L3.** See above.

## Parametrization

Read from the pinned upstream sources as data (GPL; no code copied):

- `PMSM_optimization_nonlin.m` lines 114–124 register the groups, by
  global control-point number;
- `MotorOptimizationFull.m` applies them. The bounds are stacked at lines
  58–59, and the offsets are added to the Cartesian control points along
  `atan2` of the set-up point at lines 139–155.

The set-up geometry is the generators' function defaults. For the bore that
means SW2 = 2.3 mm.

| group | upstream points | GEODE points | box (mm) |
|---|---|---|---|
| CPR1 | 43, 83 | rotor surface points on the two sector edges | [−1.5, 0] |
| CPR2 | 44, 84 | middle points of the two edge arcs (60°–68.48°, mirror) | [−1.5, 0] |
| CPR3 | 40, 47, 48, 79, 80, 50 | the 3 points of each bridge arc (68.48°–68.80°, mirror) | [−1, 0] |
| CPR4–CPR7 | 59/66, 60/65, 61/64, 62/63 | interior points of the 10-point pole arc, mirror pairs from the bridges to the pole axis | [−1.5, 0] |
| CPS1 | 18 points | the two opening corners and the opening-arc middle of each slot | [0, 0.5] |
| CPS2 | 12 points | the middle points of the two tooth arcs of each slot | [0, 0.5] |
| CPS3 | 7 points | the tooth-centre bore points at 60°, 70°, …, 120° | [0, 0.5] |

The upstream global numbers come from the GeoPDEs multipatch numbering. That
code is not in the pinned snapshot, and the numbering was not reproduced.
Simple patch-sequential and interface-first numberings of the export did
not give these numbers. The groups are therefore identified from the data:

- the group sizes and bounds;
- the mirror symmetry;
- the iteration-120 export, which differs from the scalar-only
  construction by one offset per group along the frozen directions, to
  round-off.

The other stator assignment, with the tooth-arc middles and the opening
middle in one group, leaves a 3.0e-8 m misfit. The order of the labels
CPR4..CPR7, and of CPR1 against CPR2, follows the contiguous upstream
numbers. The fit and the bounds do not depend on that order.

Symmetry and antiperiodicity follow from the ties. Every group is symmetric
about the pole axis. `CPR1` and `CPS3` each contain both antiperiodic edge
points, so the radial edges stay `R(60°)` images after motion (4.7e-16 and
7.2e-16 m on L0).

**Curves and motion.** These pieces become rational B-splines through the
moved control points (`CurveShape::Spline`):

- the rotor surface, as five pieces. The pole arc is one rational quadratic
  arc refined by inserting 7 uniform knots; at θ0 it matches the export to
  5e-16 m;
- the three bore arcs of each slot.

The opening walls and both radial edges of rotor and stator end at the
moved points. Nodes on a spline keep their knot parameter.

The interior extension is the #1176 one. Its default here is the plain
harmonic extension (`χ = 0`, `CP_MOTION_DEFAULT`) with sliding stator
edges, and the rotor edges keep a prescribed stretch. Both choices were
measured:

- with the #1176 Jacobian stiffening (`χ = 1`), the full iteration-120 design
  inverts triangles from L1 on (table below);
- sliding rotor edges folded the iron next to the `CPR1` corner at L1
  (from about −1.44 mm) in a development run that is not committed; with
  the stretch, `CPR1` covers its whole box (table in `tables.md`).

**Air gap.** The coupling circles never move: every Jacobian column is
exactly zero on them, and their positions are bit-identical at every box
end. The inner band (tag 4) moves only with the 7 rotor groups. The outer
band (tag 5) moves only with SW2 and the 3 stator groups. The #1176 rule
that tag 4 never moves holds for the scalars only, because the free-form
rotor surface is the band's inner boundary. The 9-parameter map of #1176 is
unchanged bit for bit: `runs/fd_L0.json` and the quality records of
`../parametric/` reproduce byte for byte after the refactor.

## Results

`tables.md` is generated by `analyze.py` and holds every table and every
per-run gradient table. The main ones are below.

**Full iteration-120 design, coupling torque** (N m per sector, saved
ScaleR and LENGTH):

| current | level | moved (cp) mean / std | fresh full mean / std | moved (9 scalars) mean | reference mean / std | max abs(moved − fresh) |
|---|---|---|---|---|---|---|
| 0.5 | L0 | 0.75070 / 0.00057 | 0.75077 / 0.00050 | 0.68061 | 0.74975 / 0.00044 | 2.5e-4 |
| 0.5 | L1 | 0.74902 / 0.00056 | 0.74894 / 0.00055 | 0.67842 | | 2.6e-4 |
| 0.5 | L2 | 0.74862 / 0.00056 | 0.74866 / 0.00055 | 0.67781 | | 6.8e-5 |
| 1 | L0 | 1.50298 / 0.00059 | 1.50330 / 0.00043 | 1.33868 | 1.50001 / 0.00018 | 7.2e-4 |
| 1 | L1 | 1.49906 / 0.00028 | 1.49937 / 0.00027 | 1.33296 | | 5.0e-4 |
| 1 | L2 | 1.49857 / 0.00030 | 1.49873 / 0.00031 | 1.33202 | | 2.4e-4 |

At zero current the moved std is 0.00052, 0.00062 and 0.00062 N m (L0 to
L2), against 0.00061, 0.00067 and 0.00063 for the fresh mesh and 0.00066
saved. The remaining differences to the saved arrays are of the size of the
reference's own discretization, as in #1176. This does not bound the
reference's error.

**Full iteration-120 design on the moved mesh, per extension variant:**
min angle and inverted triangles.

| level | slide, χ 0 (default) | slide, χ 0.5 | slide, χ 1 | stretch, χ 0 |
|---|---|---|---|---|
| L0 | 12.43°, 0 | 12.83°, 0 | 9.78°, 0 | 12.14°, 0 |
| L1 | 2.18°, 0 | 0.29°, 2 | 1.88°, 4 | 2.18°, 0 |
| L2 | 1.33°, 0 | 1.58°, 4 | 1.02°, 11 | 0.70°, 1 |
| L3 | 5.44°, 4 | 2.08°, 20 | 0.29°, 57 | 0.15°, 10 |

In every variant the inversions sit at the pockets' outer corners
(r ≈ 42.0 mm, 68.8° and 111.2°). These are re-entrant corners of the iron,
where a harmonic extension has a singular displacement gradient. `CPR3`
thins the bridge above them from 2 to 1 mm. The L2 moved mesh is valid,
but its smallest area ratio is 0.016.

**Gradient validation, summary over the 10 offsets** (Jinit, full current
unless noted; worst best-FD relative error, and narrowest widest O(h²)
window in decades):

| level | design | current | mean: worst FD | std: worst FD | mean: window | std: window |
|---|---|---|---|---|---|---|
| L0 | θ0 | 0.5 / 1 | 5.7e-9 / 3.8e-9 | 6.2e-8 / 1.5e-8 | 5.5 / 5.5 | 5.0 / 5.5 |
| L0 | opt120 | 0.5 / 1 | 1.6e-8 / 1.1e-8 | 6.3e-7 / 6.7e-7 | 5.0 / 5.0 | 4.5 / 3.5 |
| L1 | θ0 | 0.5 / 1 | 4.7e-9 / 9.7e-9 | 3.2e-8 / 3.6e-8 | 5.0 / 5.5 | 5.0 / 5.0 |
| L1 | opt120 | 0.5 / 1 | 2.7e-8 / 4.6e-9 | 4.9e-7 / 1.4e-6 | 5.0 / 5.5 | 4.5 / 4.0 |
| L2 | θ0 | 1 | 5.7e-9 | 1.6e-7 | 4.0 | 4.0 |
| L2 | opt120 | 1 | 3.0e-8 | 8.6e-7 | 4.0 | 4.0 |

At θ0 every offset sits at one end of its box. The step `h = 1e-3` m is
rejected there, because the far side crosses the coupling circle. At L2 at
iteration 120, `1e-4` m is also rejected for `CPR3` and `CPR4`, because a
triangle inverts. The map is smooth across the box ends, so central
differences there are a property of the map, not of a feasible design.

## Method and files

| file | content |
|---|---|
| `run_all.sh` | every stage: `geometry`, `meshes`, `torque`, `gradient`, `remesh`, `test`, `analyze` |
| `cp_table.json` | the 55 control points: group, frozen direction, positions at θ0 and at the iteration-120 scalars (`icem_cp_parametric table`) |
| `fit_offsets.py` | `fixture`: locates the table points in the hash-pinned exports and does an independent Python fit. `boundary`: the two-sided boundary check of `../parametric/check_boundaries.py` with NURBS curves |
| `opt120_control_points.json` | the export control points (fixture for CI) |
| `opt120_offsets.json` | the Rust least-squares fit (`icem_cp_parametric fit`): offsets, residuals, bound check |
| `curves_cp.json`, `boundary_check_cp.json` | the construction at θ0 and at iteration 120, and its boundary check |
| `runs/sweep_L{0,1}.json`, `runs/fd_L{0,1}.json`, `runs/quality_*.json` | quality over the box, dX/dθ against central differences (worst 2.3e-9 for the scalars and 1.5e-13 for the offsets, which enter affinely), extension variants |
| `runs/opt120cp_L{0,1,2}_h45.json` | torque records of the moved full iteration-120 meshes |
| `runs/gradient_L*_*.json`, `runs/remesh_L*.json` | gradient studies and the remeshed cross-check (`icem_cp_gradient`) |
| `tables.md`, `summary.json` | generated by `analyze.py` |
| `logs/*.log` | console output of every stage and of the two CI tests (`--nocapture`) |

Code:

- the module `crates/geode-core/src/mesh/icem_controlpoints.rs`. It shares
  the attachment and extension core of `icem_parametric.rs`, which is now
  `MotionCore`, with `Dual` generalized to `DualN<N>`;
- the examples `icem_cp_parametric` and `icem_cp_gradient`;
- `tests/common/icem_cp_design.rs`;
- the CI tests `motor_icem_controlpoints`, which needs no upstream files,
  and `motor_cp_design_gradient`, an L0 slice of the gradient study;
- `../../motor_shape_gradient/design/build_design_mesh.py`, which now also
  meshes NURBS plans. Its line/arc output is unchanged: the #1177 θ0 and
  MW L0 meshes reproduce their recorded node and triangle counts.

## Reproduce

From the repository root, with the upstream cache fetched
(`../extract_reference.py --fetch`) and a private `CARGO_TARGET_DIR`:

```sh
benchmarks/motor_reference/parametric_cp/run_all.sh          # all stages
cargo test -p geode-core --release --test motor_icem_controlpoints --test motor_cp_design_gradient
```

## Provenance and license

The exports are upstream data (GPL-3.0-or-later; see `../UPSTREAM_LICENSE.txt`).
`opt120_control_points.json` is derived from them. The group numbers are
data from the pinned optimization script. The construction, the fit, the
node motion and the scripts are original GEODE code.
