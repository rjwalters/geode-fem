# ICEM sector torque: discrete shape adjoint (M3b)

Motor paper M3, part B, issue #1174. This adds the discrete shape adjoint
of the nonlinear sector torque with respect to the mesh node coordinates
`X`, for both rotor-stator couplings, and for the mean and the
population-std (ripple) objectives over an angle set. It is validated on
generic smooth node velocities `V`, so it does not depend on the design
parametrization of part A (#1173).

**Verdict (measured, mesh L0, rel_tol 1e-14).**
- **Taylor tests.** The remainder `|J(X+hV) − J(X) − h dJ·V|` is O(h²)
  across both coupling modes, all three currents and both objectives. In
  14 of 18 cases at the initial design `X0`, and in 14 of 18 at a second
  design `X1`, the O(h²) window spans at least 5 decades of h. That window
  is the range of h over which `remainder / h²` stays within a factor 2 of
  its value at h = 1e-5 m; it typically runs from h = 3e-3 m or 1e-3 m
  down to h = 1e-8 m.
- **Where it falls short.** The cases below 5 decades are the zero-current
  mean (2.0 to 4.0 decades at X0, 0.5 to 3.5 at X1) and two full-current
  std cases (4.5 decades each). The likely causes are diagnosed
  [below](#diagnosis-of-the-short-windows). The check was not loosened.
- **Explicit Arkkio terms.** A velocity that also moves the band nodes off
  the coupling circle (the rotor surface at 44.0 mm and the stator bore at
  45.0 mm) gives O(h²) windows of 6.0 to 6.5 decades in all 18 cases. Here
  the explicit `∂T/∂X` is large: at 0 deg and full current it is +80.8 of
  a total of −47.4 N m/m.
- **Central differences.** At the best step, the central difference agrees
  with the adjoint to between 3.6e-12 and 4.5e-8 relative. The largest
  errors are on the zero-current mean, whose gradient is about 100 times
  smaller than the others.
- **Tolerance stability.** At the default rel_tol 1e-8 the gradient differs
  from the rel_tol 1e-14 gradient by at most 4.6e-7 relative under load,
  and by up to 4.8e-5 for the zero-current mean. At rel_tol 1e-12 the
  difference is at most 5.8e-9.
- **Cost.** One adjoint (factorization, saddle solve and gradient
  assembly) takes 0.105 to 0.187 of the time of the forward solve it
  belongs to in a warm-started 10-angle sweep, and 0.03 to 0.12 of a
  cold-started forward solve.

The full tables are in [`tables.md`](tables.md), condensed in
[`summary.md`](summary.md); the raw data are in `study.json`.

## Method

`crates/geode-core/src/assembly/magnetostatic_shape.rs`
(`SectorShapeProblem`); the module documentation has the derivation.

- **Element shape derivatives.** P1 triangles have exact first-order shape
  derivatives: `∂|T|/∂x_q = |T| g_q` and `∂g_p/∂x_{q,c} = −g_{p,c} g_q`.
  These use the true gradients and the unsigned area, as every assembler
  term has since #1166/#1167. Everything else is built from these two
  identities in closed form; no finite difference enters.
- **Residual.** `ψᵀ ∂R/∂X` is the element tensor
  `|T| [ν (Ψ·G) I − ν (GΨᵀ + ΨGᵀ) − 2ν' (Ψ·G) GGᵀ] g_q`, minus the load
  terms.
  - **Current.** J = I·N·sign / A_g, where the group area A_g is the
    **mesh area at the current node positions**: the area moves with X.
    This matches `icem_forward` (`AreaMode::Mesh`) and the reference's
    prescribed current: when a slot grows, its current density falls.
    `GroupAreaRule::Fixed` holds J fixed instead.
  - **Magnetization.** `∫ M·(∇φ)^⊥` with a per-triangle M that does not
    depend on X.
- **Torque functionals.**
  - **Arkkio** over both gap bands, written with the centroid as
    `|T| (B·c)(B·c^⊥)/|c|`. It includes its explicit X terms.
  - **Harmonic coupling torque** `−L λᵀ ∂(Gᵀ A)/∂β`. It has no explicit X
    term because the coupling circle cannot move. The rows of
    `∂(Gᵀ A)/∂β` come from the new
    `SlidingBand::harmonic_coupling_torque_rows`, and the CI test checks
    them against `harmonic_coupling_torque`.
- **Adjoint.** The adjoint solves the converged saddle system
  `[[K, C], [Cᵀ, 0]] [ψ; χ] = −[Pᵀ ∂T/∂A; ∂T/∂λ]` with the forward's
  consistent tangent K and the harmonic constraint matrix C (C is empty
  for the conforming band). The multiplier block goes through the dense
  16 x 16 Schur complement. The gradient is
  `dT/dX = ∂T/∂X + (Pψ)ᵀ ∂R/∂X`. There is one adjoint per angle and
  current. At step k the terms are evaluated on the rotated mesh, and the
  rotor rows are rotated back, so every gradient is with respect to the
  reference (k = 0) coordinates.
- **Objectives.** `AngleSetGradient` gives the gradients of the mean and
  of the population std, `dσ = Σ_j (T_j − T̄) dT_j / (n σ)`, over the
  reference's optimization grid 0, 2, ..., 18 deg (`conventions.json`
  `torque.optimization_grid_mechanical_degrees` and
  `ripple_definition`). The reference objective's sum over the three
  currents is a sum of these.
- **API for part A.** `SectorShapeProblem::design_gradient(grad, &[V_k])`
  returns `dJ/dθ_k = ⟨dJ/dX, V_k⟩` for split-mesh velocities
  `V_k = ∂X/∂θ_k`, and `expand_velocity` maps an input-mesh velocity to
  the split mesh. A velocity that moves the coupling circle is rejected.
- **Moved designs.** These are evaluated through
  `SlidingBand::with_node_positions`, on the same topology, links and
  coupling grid. It rejects a moved coupling-circle node (the air-gap rule
  of #1173) and an inverted triangle. The forward path at `X0`
  reproduces the `icem_forward` model with a bit-identical `A_z` and with
  Arkkio and coupling torques equal to 1e-12 (CI test).

### Test velocities

`tests/common/icem_shape.rs` `smooth_velocity`:
- **Form.** `V = v_r e_r + v_θ e_θ`, with four random modes in
  `cos/sin(6mθ)` times `cos(πm s)`. The modes are 60°-periodic in θ, so the
  radial-edge pairs stay exact rotations of each other.
- **Cut-off.** A radial cut-off `4s(1−s)` vanishes on the Dirichlet
  circles and on the band. The velocity is set exactly to zero on every
  node of the tag 4/5 triangles and on the coupling and Dirichlet nodes.
- **Scale.** `max |V| = 1`, so h is the largest node displacement in
  metres. Every step up to h = 3e-3 m (3 mm) gives a valid mesh; none was
  rejected.
- **Variants.**
  - The second design is `X1 = X0 + 2e-4 m · V(seed 7)`.
  - The band-moving variant cuts off at the coupling circle instead of the
    band, so it moves the 44.0 and 45.0 mm circles.

Newton runs at rel_tol 1e-14 for the Taylor and FD studies. At rel_tol
1e-12 the remainders flattened at about 1e-13 N m. That is the Newton
tolerance, not the gradient: the cold-versus-warm re-solve difference was
1.1e-13 on the mean at 1e-12. At 1e-14 the floor is 1e-16 to 2.5e-14 N m
(the noise columns in `summary.md`). In a check at full current
(conforming), rel_tol 1e-16 gave the same tables as 1e-14 to the printed
digits.

## Results

### Taylor windows (X0, off-band velocity)

| mode | current | mean: window, decades | mean: local orders | std: window, decades | std: local orders |
|---|---|---|---|---|---|
| conforming / Arkkio | 0 | 3e-4 .. 3e-8, 4.0 | 1.73 .. 2.05 | 3e-3 .. 3e-9, 6.0 | 1.58 .. 2.11 |
| conforming / Arkkio | 0.5 | 3e-3 .. 1e-8, 5.5 | 1.89 .. 2.03 | 3e-3 .. 3e-9, 6.0 | 1.61 .. 2.06 |
| conforming / Arkkio | 1 | 3e-3 .. 1e-8, 5.5 | 1.96 .. 2.31 | 1e-3 .. 1e-8, 5.0 | 1.68 .. 2.18 |
| h45 / coupling torque | 0 | 3e-5 .. 3e-7, 2.0 | 1.85 .. 2.03 | 1e-3 .. 1e-8, 5.0 | 1.61 .. 2.07 |
| h45 / coupling torque | 0.5 | 3e-3 .. 1e-8, 5.5 | 1.89 .. 2.31 | 3e-3 .. 1e-8, 5.5 | 1.49 .. 2.12 |
| h45 / coupling torque | 1 | 3e-3 .. 1e-9, 6.5 | 1.70 .. 2.33 | 1e-3 .. 3e-8, 4.5 | 1.81 .. 2.13 |
| h45 / Arkkio | 0 | 1e-4 .. 3e-8, 3.5 | 1.81 .. 2.46 | 1e-3 .. 1e-8, 5.0 | 1.99 .. 2.07 |
| h45 / Arkkio | 0.5 | 3e-3 .. 1e-8, 5.5 | 1.89 .. 2.18 | 3e-3 .. 1e-8, 5.5 | 1.74 .. 2.12 |
| h45 / Arkkio | 1 | 3e-3 .. 1e-8, 5.5 | 1.97 .. 2.33 | 1e-3 .. 1e-8, 5.0 | 1.82 .. 2.17 |

**How to read the table.**
- **Coupling modes.** h45 is the reference's harmonic coupling, n = 3:6:45
  (tied ends). "h45 / Arkkio" differentiates the Arkkio torque of the
  harmonic solve, which goes through the multiplier block of the adjoint
  too.
- **Local orders.** The orders are local, between neighbouring steps.
  Inside the windows, the excursions from 2 come from the ends of the
  windows: higher-order terms at the top and the round-off floor at the
  bottom.
- **Saturation.** Full current is saturated, with iron |B| up to 2.66 T
  on these angles.

**Example tables.** At full current in the conforming mode (from
`tables.md`), `remainder / h²` for the mean is 925 to 974 N m/m² from
h = 1e-3 to 3e-8 m. For the std it is 52.6 to 67.8.

**At X1** the windows are 5.0 to 6.5 decades in 14 of 18 cases. The
exceptions are the zero-current mean (0.5, 3.5 and 2.0 decades) and the
full-current std of h45/Arkkio (4.5). **With the band-moving velocity**
all 18 windows span 6.0 to 6.5 decades.

### Central-difference step sweep (X0, h = 1e-3 .. 1e-10 m)

| mode | current | mean: best rel. err (h) | std: best rel. err (h) |
|---|---|---|---|
| conforming / Arkkio | 0 | 3.3e-8 (1e-7) | 3.4e-9 (1e-7) |
| conforming / Arkkio | 0.5 | 1.4e-10 (1e-7) | 1.4e-9 (1e-8) |
| conforming / Arkkio | 1 | 4.1e-11 (1e-7) | 5.8e-10 (1e-7) |
| h45 / coupling torque | 0 | 4.5e-8 (1e-6) | 2.0e-9 (1e-6) |
| h45 / coupling torque | 0.5 | 3.6e-12 (1e-7) | 5.9e-9 (1e-7) |
| h45 / coupling torque | 1 | 7.4e-10 (1e-7) | 2.3e-10 (1e-6) |
| h45 / Arkkio | 0 | 4.0e-8 (1e-8) | 2.1e-9 (1e-6) |
| h45 / Arkkio | 0.5 | 5.0e-10 (1e-8) | 2.6e-9 (1e-7) |
| h45 / Arkkio | 1 | 3.2e-9 (1e-6) | 1.5e-9 (1e-6) |

### Gradient vs Newton tolerance

This is the largest relative change of `dJ·V` over three velocities,
compared with rel_tol 1e-14:

| rel_tol | under load (currents 0.5, 1), max | zero current, max |
|---|---|---|
| 1e-6 | 4.8e-5 | 6.3e-3 (mean), 2.5e-4 (std) |
| 1e-8 | 4.6e-7 | 4.8e-5 (mean), 1.3e-6 (std) |
| 1e-10 | 7.9e-10 | 9.4e-6 (mean), 1.8e-9 (std) |
| 1e-12 | 7.1e-11 | 5.8e-9 (mean), 8.6e-11 (std) |

The change falls roughly in proportion to the tolerance. At zero current
the mean is a cogging average, about 1e-6 N m. Its gradient is about 100
times smaller than under load, so the same absolute error is a larger
relative one.

### Cost of an adjoint

The ratio is adjoint wall time over forward wall time for the same
operating points. Apple M3 Ultra, one process, sequential:

| mode | warm 10-angle sweep: forward / adjoint (s) | ratio | cold single solve at 0 deg: forward (Newton its) / adjoint (s) | ratio |
|---|---|---|---|---|
| conforming / Arkkio | 1.41-1.58 / 0.20-0.28 | 0.13-0.18 | 0.26-1.23 (10-13) / 0.02-0.04 | 0.03-0.12 |
| h45 / coupling torque | 0.81-1.65 / 0.10-0.31 | 0.105-0.187 | 0.14-0.16 (11-13) / 0.010 | 0.06-0.07 |
| h45 / Arkkio | 0.64-0.74 / 0.10 | 0.13-0.15 | 0.13-0.15 (11-13) / 0.010 | 0.06-0.07 |

**Why the adjoint is cheap.** It is one sparse LU of the converged
tangent, plus 17 triangular solves in the harmonic mode. The forward is a
Newton solve with one LU per iteration and a line search. The warm sweep
at rel_tol 1e-14 takes 50 to 60 Newton iterations over the 10 angles; a
cold solve takes 10 to 13.

**Caveats.**
- The adjoint factors the tangent again instead of reusing the forward's
  last factorization; reusing it would lower the ratio further.
- These are wall times of a 3153-node mesh, where fixed overheads are
  visible. The 1.23 s cold forward at full current in the conforming mode
  is an outlier against the 0.26 to 0.36 s of the other currents.

## Diagnosis of the short windows

**Zero-current mean (2.0 to 4.0 decades at X0, 0.5 to 3.5 at X1).**
- **The objective is small.** With no current, the torque over the 10
  angles is pure cogging, and its mean is 5e-6 N m or less. Its gradient
  (about 0.2 N m/m along V) and its second-order coefficient (0.4 to 2.5
  N m/m²) are about 100 and 1000 times smaller than those of the mean
  under load (14 to 29 N m/m and 370 to 990 N m/m²).
- **Top end.** The h² term is small, so the h³ and higher terms compete
  with it from h ≈ 1e-4 m upward. `remainder / h²` is then not monotone,
  and in the conforming X1 table `J(X+hV) − J(X) − h dJ·V` changes sign
  twice between h = 3e-5 and 1e-3, and again at 3e-3.
- **Bottom end.** The remainder reaches the round-off floor (about 1e-15
  N m) near h = 1e-7.
- **Between the ends** the local order is 1.8 to 2.4. The first-order term
  agrees with a central difference to 3e-8 to 4.5e-8. So the evidence
  points to the objective's small curvature narrowing the window, not to
  a gradient error. It is not proven: a curvature this small cannot be
  separated more cleanly in f64.

**Full-current std (4.5 decades: h45/coupling at X0, h45/Arkkio at X1).**
- **Top end.** The window stops at h = 1e-3 m, because at 3e-3 m (3 mm
  of node motion) the higher-order terms exceed a factor 2.
- **Bottom end.** It stops at 3e-8 m, where the remainder (about 5e-14)
  meets the 1.6e-14 to 1.8e-14 noise of the std.
- **Possible remedy (untried).** The window can only widen if the noise
  floor drops. At the tightest Newton setting it is already at f64
  round-off, so this would take extended-precision torque sums.

## Reproduce

```text
benchmarks/motor_shape_gradient/run_all.sh
```

The script:
- builds and runs `examples/icem_shape_gradient.rs` at rel_tol 1e-14,
  which writes `tables.md`, `study.json` and `logs/study.log`;
- runs the CI test (`logs/test_motor_shape_gradient.log`) and the
  element-level unit tests (`logs/test_magnetostatic_shape_unit.log`);
- writes `summary.md` and `logs/analyze.log` with `analyze.py`.

`logs/test_existing_magnetostatic.log` holds the existing forward tests,
unchanged and passing: `magnetostatic_nonlinear`,
`magnetostatic_sector_sliding`, `magnetostatic_sector_harmonic`,
`motor_icem_forward` and `motor_icem_harmonic`.

## Tests in CI

`crates/geode-core/tests/motor_shape_gradient.rs`, in the m-z list of
`integration-tests.yml`:
- the forward path reproduces `icem_forward`;
- the coupling-torque rows reproduce the band torque;
- Taylor tests (`remainder / h²` within 25% over h = 1e-4 .. 1e-7 m, and
  central FD ≤ 1e-7) at full current (conforming, Arkkio) and half
  current (h45, coupling torque);
- the air-gap rule and velocity admissibility;
- `design_gradient` linearity.

The lib unit tests in `magnetostatic_shape.rs` check the element-level
`ψᵀ ∂R/∂X` against central differences. They cover a Brauer law, both
group-area rules and a magnetization. They also check the Arkkio `∂T/∂A`
and `∂T/∂X`.
