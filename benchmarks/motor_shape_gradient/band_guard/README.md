# Arkkio band-area guard (issue #1183)

The Arkkio torque `L/(μ₀(r_o − r_i)) ∫ r B_r B_θ dA` is the air-gap torque
only if the integration region is the annulus `[r_i, r_o]` it is normalised
by. The free-form rotor offsets of the iteration-120 design (Opt120) move the
rotor surface from 44.0 mm in to about 42.6 mm. The new air gap keeps the
inner gap-band tag (4), so it is no longer the annulus. On Opt120 the
both-band Arkkio gives 2.30 N m against about 1.31 N m at L3
(`benchmarks/motor_validation/`, #1182).

## Behaviour

`SectorShapeProblem::solve` and `SectorShapeProblem::torque_gradient` (and so
`solve_angle_set` and `angle_set_gradient`) share one check
(`crates/geode-core/src/assembly/magnetostatic_shape.rs`, `check_torque`).
For a `ShapeTorque::Arkkio`, the terms are grouped by annulus (bitwise-equal
radii; the two-band ICEM functional, tags 4 and 5 both normalised by
44.0–45.0 mm, is one group). The summed mesh area of each group is compared
with `(sector_angle / 2)(r_o² − r_i²)`. If any group departs by more than
`ARKKIO_BAND_AREA_TOL = 1e-3` (relative), both calls return
`SectorError::InvalidInput`. The message names the annulus, the tags and the
ratio.

The check is a **loud error, not an automatic switch to the outer band**:

- switching would change the objective's definition mid-optimization, with a
  jump where the design crosses the tolerance;
- the caller picks a functional that stays valid, and can query
  `SectorShapeProblem::arkkio_band_areas` first. That is either the outer
  band alone (`icem_shape::outer_arkkio_spec()`, tag 5 over 44.5–45.0 mm,
  between the fixed coupling circle and the stator bore) or the coupling
  torque.

The outer band keeps its annulus only under the **rotor** offsets. The
stator offsets `CPS1`/`CPS2`/`CPS3` at their bounds move the bore and put
the outer band 0.535/0.233/0.239 off its annulus. With stator offsets in
conforming mode, neither Arkkio functional is valid; only the coupling
torque (harmonic coupling) remains.

**Outer-band shear bias.** Even where the guard accepts it, the outer-band
Arkkio on a moved mesh is **not a calibrated torque**. Whenever `SW2`
differs from the meshed design, moving the stator-bore nodes tangentially
shears the outer band's triangles (as #1176 found). The value then sits
about 1% below the coupling torque, and the gap does not shrink with mesh
level:

| Opt120 h45 mean gap | L0 | L1 | L2 |
|---|---|---|---|
| full design | −1.03% | −1.10% | −0.86% |
| `SW2` alone | −1.21% | −1.14% | −0.85% |
| rotor offsets alone | about 1e-4 | | |

The 1.3e-4 N m agreement in `benchmarks/motor_validation/` (#1182) is on
fresh meshes. The adjoint differentiates the sheared functional exactly,
so its gradient matches finite differences, but the functional is
consistently biased.

The tolerance sits far from both sides:

- at L0 the both-band group departs from its annulus by `3.7e-5` and the
  outer band by `8.4e-5` (P1 chords). On the initial-design meshes this
  shrinks with refinement. On a moved mesh it is set by the design, not by
  the level;
- Opt120 departs by `0.615` (both bands) and `1.237` (inner band).

The tolerance is pinned by Opt120's offsets scaled down:

| offset scale | both-band departure | result |
|---|---|---|
| ×0.001 | 5.9e-4 | accepted |
| ×0.003 | 1.84e-3 | rejected |
| ×0.01 | 6.19e-3 | rejected |

Moving `ARKKIO_BAND_AREA_TOL` out of [5.9e-4, 1.84e-3) fails the test.

The raw estimators `torque::arkkio_torque` and the test helper
`IcemModel::arkkio` are not guarded. They are diagnostics, and
`examples/icem_validation.rs` records `gap_*band_area_ratio` next to them so
the analysis marks invalid values.

## Results (L0, `logs/`)

- `test_motor_arkkio_band_guard.log`
  (`tests/motor_arkkio_band_guard.rs`):
  - band ratios: θ0 both-band 1.000036670, outer 1.000084028; Opt120
    both-band 1.615160190, outer 1.000085368;
  - Opt120, harmonic n = 3:6:45 and conforming: forward solve, angle-set
    solve and gradient all reject the both-band functional;
  - θ0 still accepts it with the unguarded value (round-off);
  - the tolerance pin above (offset scales 0.001/0.003/0.01);
  - Opt120, harmonic, saved full-load current: outer-band Arkkio 0.99–1.13%
    below the coupling torque at every angle of the 0..18° grid. This is
    the `SW2` shear bias above, not L0 coarseness. The test bounds it at
    2%, which is not an accuracy claim. The unguarded both-band sum is
    2.99 N m against 1.71 N m;
  - mean-torque gradient over the seven rotor offsets `CPR1..CPR7` (all move
    inner-band nodes): adjoint against a central difference at `h = 1e-8` m
    that re-moves the mesh, at most 5.8e-7 relative (harmonic) and 1.2e-7
    (conforming). Against the coupling-torque gradient, at most 1.35e-2
    relative, the same shear bias (bounded at 3%).
- `test_magnetostatic_shape_unit.log`: the unit test
  `arkkio_band_areas_group_and_flag_a_moved_boundary` on a banded disk
  (grouping, a repeated band, a moved inner boundary).
- `mutation_checks.log`: five mutations, each caught:
  - M1: `is_valid` always true;
  - M2: no grouping by annulus;
  - M3: `check_torque` skips the band check;
  - M4: `ARKKIO_BAND_AREA_TOL = 1e-2`;
  - M5: `ARKKIO_BAND_AREA_TOL = 1e-4`.
- `test_existing_motor.log`: the existing motor shape and design-gradient
  targets, unchanged.
- `scalar_unchanged.log`: the committed L0 conforming design studies
  (`../design/runs/study_L0_conforming_{initial,perturbed}.json`, #1177)
  re-run with the guard. Every value other than wall times matches.

## Reproduce

```text
cargo test -p geode-core --release --features spade-mesh --test motor_arkkio_band_guard -- --nocapture
cargo test -p geode-core --release --lib magnetostatic_shape
```
