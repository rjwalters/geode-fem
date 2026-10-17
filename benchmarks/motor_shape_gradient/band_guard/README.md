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

The tolerance sits far from both sides:

- at L0 the both-band group departs from its annulus by `3.7e-5` and the
  outer band by `8.4e-5` (P1 chords), less on finer meshes;
- Opt120 departs by `0.615` (both bands) and `1.237` (inner band).

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
  - Opt120, harmonic, saved full-load current: outer-band Arkkio within
    1.13% of the coupling torque at every angle of the 0..18° grid. The
    unguarded both-band sum is 2.99 N m against 1.71 N m. L0 is coarse;
    at L3 the outer band agrees to 1.3e-4 N m (#1182);
  - mean-torque gradient over the seven rotor offsets `CPR1..CPR7` (all move
    inner-band nodes): adjoint against a central difference at `h = 1e-8` m
    that re-moves the mesh, at most 5.8e-7 relative (harmonic) and 1.2e-7
    (conforming). Against the coupling-torque gradient, at most 1.35e-2
    relative at L0.
- `test_magnetostatic_shape_unit.log`: the unit test
  `arkkio_band_areas_group_and_flag_a_moved_boundary` on a banded disk
  (grouping, a repeated band, a moved inner boundary).
- `mutation_checks.log`: three mutations, each caught:
  - M1: `is_valid` always true;
  - M2: no grouping by annulus;
  - M3: `check_torque` skips the band check.
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
