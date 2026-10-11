# Refined transmon optimization (#1056, re-anchored in #1066)

This experiment optimizes the **scalar-permittivity electrostatic capacitance**
of the DeviceLayout transmon, with a neutral floating feedline, toward an
anchor. The junction's 5.5 fF lumped self-capacitance is excluded from this
capacitance. This is not an optimization of the full-wave eigenfrequency or of
the nonlinear Hamiltonian.

## The anchor (#1066)

The anchor is **93.953660 fF** (93.9537 fF). DeviceLayout publishes 4.14 GHz
for this qubit. Palace computes that number as a linear eigenmode, with the
junction as a lumped `L_J = 14.860 nH` in parallel with `C_J = 5.5 fF`, so it
is the harmonic frequency `f = 1/(2 pi sqrt(L_J (C_Sigma + C_J)))`:

| Quantity | Value |
|---|---|
| `C_total = 1/((2 pi f)^2 L_J)` | 99.453660 fF |
| `C_Sigma = C_total - C_J` (junction excluded) | **93.953660 fF** |
| `E_C = e^2/(2 C_total)` | 0.194766 GHz |

The derivation lives in one place,
`geode_core::quantum::transmon::TransmonAnchor::devicelayout`. The generator
reads the anchor from there and records it in each `summary.toml` (`[anchor]`)
and `results.json` (`anchor_ff`). `scripts/transmon_refined_palace.py` reads it
back from `results.json`.

**Superseded anchor.** The 89.9 fF used by the runs of #1056 is mis-derived.
The #505 review inverted the anharmonic `sqrt(8 E_J E_C) - E_C` at 4.14 GHz
and left out `C_J`. Those runs are kept below, unchanged, under
[Superseded runs](#superseded-runs-at-the-mis-derived-899-ff-anchor-1056).

| Run directory | Anchor | Status |
|---|---|---|
| `p2_cycles10_anchor_93p95/` | 93.9537 fF | **current** (#1066) |
| `p2_cycles10_anchor_93p95_resolution_corrected/` | 93.9537 fF | **current** (#1066), extrapolation target |
| `p2_cycles10/` | 89.9 fF | superseded: wrong anchor |
| `p2_cycles10_resolution_corrected/` | 89.9 fF | superseded: wrong anchor |
| `results.json`, `cycles16/` (exploratory) | 89.9 fF | superseded: wrong anchor |

## Result at the 93.9537 fF anchor (#1066)

`p2_cycles10_anchor_93p95/` repeats the #1056 method unchanged with the new
target:

- the same ten P1 flux-jump cycles to the same 448,279-tet mesh;
- P2 fields, exact solved shape derivatives and AMG-PCG;
- the same seven held-out rounds at the final design and at X0;
- the same rounding rules and the same extrapolation methods.

**The target is reached on that discretization, and only there.** The
capacitance goes from 78.387918461 to 93.953667887 fF in four accepted steps.
That is a move of +15.566 fF, against the +11.513 fF of #1056.

**Final design.**
- Island length 627.794327 um, pad width 30.394071 um (initially 625.5 and
  24 um).
- theta = [0.0037489, 0.2664196, -0.2980496] in the three parameter fields
  (length, width, gap).

**Mesh validity.** The minimum element-volume ratio over the accepted steps is
0.47801 (step 2), and the final ratio is 0.50657. Both are rounded down; the
floor is 0.25. The larger move costs volume ratio (#1056 had 0.61413 and
0.61948), but every accepted step stays far above the floor.

**Gradient checks.** All three solved finite-difference checks agree with the
analytic gradient. The largest relative discrepancy is 4.15e-7.

| Round | Tets | Final design C (fF) | Decrement (fF) | X0 C (fF) | Increment (fF) |
|---|---|---|---|---|---|
| 0 (optimization mesh) | 448,279 | 93.953667887 | | 78.387918461 | 15.565749426 |
| 1 | 527,959 | 93.698450413 | 0.255217 | 78.178674518 | 15.519775895 |
| 2 | 683,440 | 93.488379452 | 0.210071 | 78.011681063 | 15.476698390 |
| 3 | 902,171 | 93.332105806 | 0.156274 | 77.882134863 | 15.449970944 |
| 4 | 1,250,525 | 93.214010250 | 0.118096 | 77.784537640 | 15.429472610 |
| 5 | 1,720,753 | 93.119307099 | 0.094703 | 77.707881808 | 15.411425291 |
| 6 | 2,430,808 | 93.052517479 | 0.066790 | 77.651593966 | 15.400923513 |
| 7 | 3,388,197 | 92.998560071 | 0.053957 | 77.607977900 | 15.390582171 |

**Rigorous.**
- The held-out spaces are nested and conforming, and the capacitance is an
  energy minimum. Every value is therefore an upper bound on the converged
  capacitance of this polygonal geometry.
- The converged value is **below 93.00 fF** (round 7 rounded up). That is at
  least 1.01 % below the anchor (rounded down).
- There is no rigorous lower bound.

**Estimate, not a bound.** The decrement ratios are 0.8231, 0.7439, 0.7557,
0.8019, 0.7053 and 0.8079, with no trend toward zero.
- Geometric tail `C_7 - d_7 r / (1 - r)`, at the largest and the smallest
  observed ratio: 92.747491 to 92.869454 fF.
- Least-squares fit `C_inf + a N^-alpha` with a free rate:
  - rounds 2 to 7: 92.879735 fF (alpha = 1.004);
  - rounds 3 to 7: 92.846467 fF (alpha = 0.876);
  - rounds 4 to 7: 92.867953 fF (alpha = 0.967).
- The outward-rounded union of these is **92.74 to 92.88 fF**. The optimized
  93.9537 fF therefore overstates the estimate by 1.15 to 1.31 %. The estimate
  is 1.14 to 1.30 % below the anchor.

**The estimate depends on the assumed rate.** The same fit at a slower assumed
rate gives 92.29 fF (alpha = 1/3) and 92.58 fF (alpha = 1/2), both below the
band. That is why the band is an estimate and not a lower bound.

**Sensitivities.** From round 0 to round 7 the three shape sensitivities move by
-0.98 %, -0.94 % and -2.63 %.

**X0.**
- On these meshes, which are adapted to the final design, the undeformed
  geometry goes from 78.387918 to 77.607978 fF.
- Rigorous: below 77.61 fF.
- Estimate: 77.43 to 77.52 fF. The #1056 run gives the same bound, 77.61 fF,
  and an estimate of 77.42 to 77.52 fF, on its own meshes.
- The design increment is stable: 15.566 fF at round 0 and 15.391 fF at
  round 7.

## Resolution-corrected design at the 93.9537 fF anchor (an extrapolation target, not a certificate)

`p2_cycles10_anchor_93p95_resolution_corrected/` repeats the optimization on
the same 448,279-tet P2 discretization. Its target, **95.112986 fF**, is offset
by the bias measured above: the anchor times the round-0 value over the
geometric-tail midpoint, `93.953660 * 93.953668 / 92.808473`.

The first run's `summary.toml` records that target
(`resolution_corrected_target_ff`). The generator computed it, and it was fixed
before the second run.

The optimizer reaches **95.113016421 fF in four accepted steps**:
- island 627.924937 x 30.749537 um;
- minimum volume ratio 0.43912 over the accepted steps, 0.47763 at the end
  (both rounded down; the floor is 0.25);
- worst solved finite-difference error 4.23e-7.

| Round | Tets | C (fF) | Decrement (fF) |
|---|---|---|---|
| 0 (optimization mesh) | 448,279 | 95.113016421 | |
| 1 | 527,565 | 94.852766291 | 0.260250 |
| 2 | 681,721 | 94.637931574 | 0.214835 |
| 3 | 900,507 | 94.477742135 | 0.160189 |
| 4 | 1,248,566 | 94.355732401 | 0.122010 |
| 5 | 1,723,255 | 94.259622572 | 0.096110 |
| 6 | 2,433,998 | 94.191268088 | 0.068354 |
| 7 | 3,395,664 | 94.136103400 | 0.055165 |

- **Rigorous.** The converged value of this geometry is below 94.14 fF, at
  most 0.20 % above the anchor (rounded up). The last solved value is still
  above the anchor, so nothing rigorous places the converged value near it.
- **Estimate, not a bound.**
  - The geometric tail gives 93.875150 to 94.000247 fF (midpoint 93.937698 fF).
  - The free-rate fits give 94.016533, 93.983604 and 93.996598 fF.
  - The band is **93.87 to 94.02 fF**. It contains the anchor and is within
    0.09 % of it on either side.
- **Rate dependence.** At a slower assumed rate the series extrapolates to
  93.41 fF (alpha = 1/3) or 93.71 fF (alpha = 1/2), both below the anchor.

This design is tuned so that an *extrapolation* lands on the anchor. The
method was chosen on the #1056 runs and is used here unchanged. But all runs
share the same mesh family, marking rule and element order, so the agreement
is not an independent confirmation. A continuum statement needs one of:
- an error estimator with a guaranteed lower bound (#1062 is building a dual,
  equilibrated-flux one; it has not landed);
- a substantially finer or higher-order solve.

Neither is here.

## Frequency side (observation, #1066)

What each number implies for the harmonic qubit frequency
`1/(2 pi sqrt(L_J (C_Sigma + C_J)))`. The frequency falls as `C_Sigma` rises,
so:
- a rigorous upper bound on `C_Sigma` is a rigorous lower bound on the
  frequency (rounded down);
- estimate bands are rounded outward.

Each `summary.toml` carries these numbers in a `[*.qubit]` table, together
with `E_C = e^2/(2 (C_Sigma + C_J))`. They are reported as observations; the
gap is not explained here.

| Geometry | `C_Sigma` upper bound (fF) | Frequency at least (GHz) | Estimate band (fF) | Frequency estimate (GHz) |
|---|---|---|---|---|
| #1066 design (4.14 GHz on the optimization mesh) | 93.00 | 4.1599 | 92.74 to 92.88 | 4.1625 to 4.1655 |
| #1066 resolution-corrected design | 94.14 | 4.1361 | 93.87 to 94.02 | 4.1386 to 4.1418 |
| X0 (#1066 meshes) | 77.61 | 4.5288 | 77.43 to 77.52 | 4.5312 to 4.5338 |
| X0, tensor eps (#1051, uniform refinement) | 89.50 | 4.2359 | 76.61 to 81.75 | 4.4200 to 4.5564 |
| X0, scalar eps (#1051, uniform refinement) | 83.02 | 4.3882 | 74.14 to 79.62 | 4.4750 to 4.6265 |
| #1056 design (superseded 89.9 fF target) | 89.02 | 4.2466 | 88.77 to 88.91 | 4.2491 to 4.2524 |

DeviceLayout publishes 4.14 GHz for X0 on its own mesh. On these meshes:
- the converged X0 geometry has a harmonic frequency of at least 4.2359 GHz
  (tensor eps) or 4.5288 GHz (scalar eps);
- the #1066 design, which sits on 4.14 GHz on the optimization mesh, converges
  to at least 4.1599 GHz.

`tests/transmon_diffopt.rs` (`committed_bounds_imply_harmonic_frequencies` and
the two #1066 pin tests) recomputes every entry of this table from the
committed bounds.

## Cross-solver check at the 93.9537 fF anchor (same discretization)

Independent Palace P2 runs on the exported 448k-tet adapted mesh:
- primary design: 93.953433803 fF (GEODE 93.953667887, -2.49e-6);
- resolution-corrected design: 95.112779561 fF (GEODE 95.113016421, -2.49e-6).

The image, the mesh SHA-256 and the commands are recorded in each
`adaptive/palace_validation.json`. This shows that the two solvers agree on the
same discretization. **It is not evidence about the discretization error**,
which the held-out series measures. No Palace P3 or AMR run was attempted at
the new anchor.

The two-sided bound of #1066 item 6 needs #1062's dual lower bound. #1062 has
not landed, so it is left as a follow-up there.

## Superseded runs at the mis-derived 89.9 fF anchor (#1056)

**Superseded (#1066): wrong anchor.** Everything in this section was aimed at
89.9 fF, which is not the DeviceLayout anchor (see [The anchor](#the-anchor-1066)).
The runs are kept unchanged for comparison and are not regenerated. Their
`summary.toml` files were written by the pre-#1066 generator (commit
13d4a8e8). With the current generator, `REFINED_OPT_TARGET=89.9` reproduces
their numbers, but not the layout of their summaries. The method findings
still hold, and the #1066 runs repeat them at the new anchor:
- one held-out round is not enough;
- the optimization mesh overstates the converged value by about 1.1 to 1.3 %;
- the X0 bound and estimate.

### Result (superseded)

The primary run is `p2_cycles10/` (`summary.toml` is the headline artifact,
`results.json` the full history). Ten P1 flux-jump refinement cycles prepare a
448,279-tetrahedron mesh. The optimizer then uses **P2** fields and exact
solved shape derivatives, with AMG-PCG instead of a sparse LU.

**The target is reached on that discretization, and only there.** The
capacitance goes from 78.387918461 to 89.900518266 fF in three accepted steps.
The final geometry has island length 627.323864 um and pad width 29.087261 um
(initially 625.5 and 24 um). The minimum element-volume ratio over the accepted
steps is 0.61413 (at step 2); the final ratio is 0.61948 (both rounded down;
the floor is 0.25). All three final finite-difference checks, each from two
fully re-solved perturbed geometries, agree with the analytic gradient; the
largest relative discrepancy is 3.87e-7.

**The same geometry does not hold that value under further refinement.** The
generator keeps refining the final geometry for seven held-out rounds (the
same flux-jump marking, the same exact prolongation of the geometry map, a
fresh P2 solve each round; none of it feeds the optimizer):

| Round | Tets | Final design C (fF) | Decrement (fF) | X0 C (fF) | Increment (fF) |
|---|---|---|---|---|---|
| 0 (optimization mesh) | 448,279 | 89.900518266 | | 78.387918461 | 11.512599805 |
| 1 | 528,324 | 89.662754721 | 0.237764 | 78.177697100 | 11.485057621 |
| 2 | 684,646 | 89.470365799 | 0.192389 | 78.011928175 | 11.458437624 |
| 3 | 900,226 | 89.324261426 | 0.146104 | 77.882040589 | 11.442220837 |
| 4 | 1,239,090 | 89.216843083 | 0.107418 | 77.786773744 | 11.430069339 |
| 5 | 1,693,481 | 89.128358967 | 0.088484 | 77.710172560 | 11.418186407 |
| 6 | 2,369,256 | 89.065595293 | 0.062764 | 77.653890383 | 11.411704911 |
| 7 | 3,289,543 | 89.014998313 | 0.050597 | 77.609944786 | 11.405053527 |

- **Rigorous.** The held-out spaces are nested and conforming and the
  capacitance is an energy minimum, so every value is an upper bound on the
  converged capacitance of this polygonal geometry. The converged value is
  **below 89.02 fF** (the round-7 value rounded up), which is at least 0.98 %
  below the anchor (rounded down). There is no rigorous lower bound.
- **Estimate, not a bound.** The decrement ratios are 0.8092, 0.7594, 0.7352,
  0.8237, 0.7093 and 0.8062, with no trend toward zero. Method 1, a geometric
  tail `C_7 - d_7 r / (1 - r)` at the largest and the smallest observed ratio,
  gives 88.778547 to 88.891530 fF. Method 2, a least-squares fit
  `C_inf + a N^-alpha` with a free rate on rounds 2 to 7, 3 to 7 and 4 to 7,
  gives 88.900718, 88.863790 and 88.892726 fF (alpha = 1.007, 0.861, 0.989).
  The outward-rounded union is 88.77 to 88.91 fF: **about 88.8 to 88.9 fF**.
  The optimized 89.9005 fF therefore **overstates the estimated converged value
  by about 1.1 to 1.3 %** (1.11 to 1.28 %).
- **The estimate depends on the assumed rate.** The same fit at an assumed
  slower rate gives 88.34 fF (alpha = 1/3) and 88.62 fF (alpha = 1/2), below
  the band. That is why the band is an estimate and not a lower bound.
- **The 1 % working threshold of #1056.** After seven rounds the capacitance is
  0.985 % below its optimization-mesh value and still falling by 0.05 fF per
  round. The rigorous bound alone (at least 0.98 %) does not decide the
  threshold. The estimate does: 1.10 to 1.26 % below the anchor. We do not
  claim the threshold is met, and we infer that it is not.
- **Sensitivities.** From round 0 to round 7 the three shape sensitivities move
  by -0.93 %, -0.67 % and -2.26 %. After one round they had moved by -0.18 %,
  -0.16 % and -0.60 %, which is what the first version of this study reported.
- **X0 and the design increment.** On the same meshes the undeformed geometry
  goes from 78.387918 to 77.609945 fF: its converged value is below 77.61 fF
  (rigorous), estimated at 77.42 to 77.52 fF. This is inside #1057's
  74.14 to 79.62 fF band and below its 83.02 fF bound. The design *increment*
  is stable: 11.513 fF at round 0 and 11.405 fF at round 7. The meshes are
  adapted to the final design, not to X0.

An earlier version of this study ran one held-out round (528,324 tets, -0.264 %)
and reported that the design "passes" the 1 % threshold. That was an artifact
of stopping after a round that adds 18 % more tetrahedra, and the claim is
withdrawn. Round 1 of the table is that solve, digit for digit.

### Resolution-corrected design at 89.9 fF (superseded)

`p2_cycles10_resolution_corrected/` repeats the optimization on the same
448,279-tet P2 discretization with the target offset by the bias measured
above: `89.9 * 89.900518 / 88.835038 = 90.978253 fF`, the anchor times the
round-0 value over the geometric-tail midpoint. `summary.toml` of the first run
records that target (`resolution_corrected_target_ff`); it was fixed before
the second run. The first run is unchanged and is kept for comparison.

The optimizer reaches 90.980103919 fF in three accepted steps (island
627.452356 x 29.447205 um, minimum volume ratio over the accepted steps 0.57790,
final 0.58759, worst solved finite-difference error 3.94e-7). Its held-out
series:

| Round | Tets | C (fF) | Decrement (fF) |
|---|---|---|---|
| 0 (optimization mesh) | 448,279 | 90.980103919 | |
| 1 | 528,677 | 90.736602272 | 0.243502 |
| 2 | 684,107 | 90.538647931 | 0.197954 |
| 3 | 905,316 | 90.391037381 | 0.147611 |
| 4 | 1,247,811 | 90.281356456 | 0.109681 |
| 5 | 1,708,342 | 90.191605939 | 0.089751 |
| 6 | 2,398,139 | 90.127899467 | 0.063706 |
| 7 | 3,331,618 | 90.076691481 | 0.051208 |

- **Rigorous.** The converged value of this geometry is below 90.08 fF, that is
  at most 0.20 % above the anchor (rounded up). The last solved value is still
  above the anchor, so nothing rigorous places the converged value near it.
- **Estimate, not a bound.** The geometric tail gives 89.846092 to 89.951431 fF
  (midpoint 89.898762 fF) and the free-rate fits 89.956997, 89.926211 and
  89.952744 fF. The band is **89.84 to 89.96 fF**, within 0.07 % of the anchor
  on either side, and it contains the anchor.
- **Rate dependence.** At an assumed slower rate the same series extrapolates
  to 89.39 fF (alpha = 1/3) or 89.68 fF (alpha = 1/2), which is 0.57 % or
  0.25 % below the anchor.

This design is tuned so that an *extrapolation* lands on the anchor. The
extrapolation method was chosen on the first run and the second run uses it
unchanged, but both runs share the same mesh family, marking rule and element
order, so the agreement is not an independent confirmation. A continuum
statement needs an error estimator with a guaranteed lower bound, or a
substantially finer or higher-order solve; neither is here.

### Cross-solver check of the superseded runs (same discretization)

- Independent Palace P2 on the exported 448k-tet adapted mesh of the primary
  design gives 89.900294115 fF, 0.000249 % from GEODE. Palace adds 3,777
  interior-boundary bisections: the same physical polygonal geometry and
  coefficients, not identical meshes. **This shows that the two solvers agree
  on the same discretization. It is not evidence about the discretization
  error**, which the held-out series above measures.
- The same check on the resolution-corrected design gives 90.979877072 fF
  against 90.980103919 fF, again 0.000249 %, and again only solver agreement.
  The `relative_target_error` the runner prints is measured against the
  89.9 fF anchor in both directories.
- The attempted Palace P3 run terminated with signal 9 during flux-error
  estimation after its first terminal solve. It produced no complete
  capacitance matrix. Its log is retained; it is **not** validation evidence.
- Palace also completed two adaptive P2 steps (452,056 to 475,079 elements).
  Its island effective capacitance remains 89.900294115 fF, while feedline
  self-capacitance changes by 1.90 %. This suggests the marking concentrated
  on the feedline; we do not count it as an island-convergence check. See
  `p2_cycles10/adaptive/palace_validation_amr.json`.

No scalar-to-tensor equivalence, CAD regeneration, full-wave optimization,
experimental validation, global optimum, or speedup against Palace is claimed.
The geometry has zero-thickness sheet conductors and a natural zero-charge
outer boundary. Mesh refinement preserves its polygonal surfaces, so
"converged" means converged for those surfaces.

## Differentiation and optimization

The coarse junction-pinned harmonic fields for island length, width and gap
are fixed. Exact nodal prolongation transports them to each refined mesh.
During optimization, X(theta) = X0 + sum(theta_i D_i); connectivity, conductor
membership and scalar material values remain fixed. Refinement occurs outside
this differentiable map. The three fixed parameter scalings are
[0.040959, 0.553015, 0.784636]; the maximum scaled step is 0.3, the volume-ratio
floor is 0.25, and accepted steps must reduce the freshly solved capacitance
residual. The discrete target tolerance is 0.005 fF.

For the equilibrated unit island excitation, the floating feedline potential
is -C_fi/C_ff. The effective field is phi_i - (C_fi/C_ff) phi_f, and its energy
is C_ii - C_if C_fi/C_ff. Stationarity eliminates implicit field derivatives.
The P1 derivative uses dual-number element differentiation. The P2 derivative
integrates div(v)|grad(phi)|^2 - 2 grad(phi)^T grad(v) grad(phi) with exact
four-point quadrature on each affine tetrahedron. The field solves and
contractions use native f64 throughout; Burn is not used in this numerical path.

Three unit tests in `shape::refined_capacitance` cover this. The P1 test
compares AMG-PCG fields with LU and the gradient with solved finite
differences. The P2 kernel test checks the frozen-field derivative. The P2
solved test (`p2_floating_sheet_energy_derivative_matches_solved_fd`) solves a
small problem with a floating sheet patch strictly inside the mesh, its
triangle-edge DOFs pinned, and compares the gradient with central differences
of fully re-solved capacitances for five velocity fields. The real-device
final checks also re-solve both perturbed geometries for every parameter.

## Reproduction

The #1066 runs, from the repository root. The first command took 29 min and
24.6 GB peak RSS, the second 12 min and 22.5 GB; both ran locally, with no
cloud. Their standard output is in
`validation_logs/anchor_93p95_*_run.log`.

```sh
REFINED_OPT_ORDER=2 REFINED_OPT_CYCLES=10 REFINED_OPT_MAX_TETS=1000000 \
REFINED_OPT_DIR=benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95 \
cargo run -p geode-core --release --example transmon_refined_optimize

REFINED_OPT_TARGET=95.112986 REFINED_OPT_HELD_OUT_X0=0 \
REFINED_OPT_ORDER=2 REFINED_OPT_CYCLES=10 REFINED_OPT_MAX_TETS=1000000 \
REFINED_OPT_DIR=benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95_resolution_corrected \
cargo run -p geode-core --release --example transmon_refined_optimize

python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95/adaptive --orders 2
python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95_resolution_corrected/adaptive \
  --orders 2
```

The superseded #1056 runs were produced by the commands below with the
pre-#1066 generator (about 20 min and up to 29.4 GB peak RSS for the first
command, about 10 min and 26.0 GB for the second). The current generator
defaults to the new anchor, so reproducing the first of them now also needs
`REFINED_OPT_TARGET=89.9`. The second sets its own target. Either way, the
current generator compares the results with the new anchor:

```sh
REFINED_OPT_ORDER=2 REFINED_OPT_CYCLES=10 REFINED_OPT_MAX_TETS=1000000 \
REFINED_OPT_DIR=benchmarks/transmon_diffopt/refined_optimization/p2_cycles10 \
cargo run -p geode-core --release --example transmon_refined_optimize

REFINED_OPT_TARGET=90.978253 REFINED_OPT_HELD_OUT_X0=0 \
REFINED_OPT_ORDER=2 REFINED_OPT_CYCLES=10 REFINED_OPT_MAX_TETS=1000000 \
REFINED_OPT_DIR=benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_resolution_corrected \
cargo run -p geode-core --release --example transmon_refined_optimize

python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/adaptive --orders 2
python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/adaptive \
  --orders 2 --amr-steps 2
python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_resolution_corrected/adaptive \
  --orders 2

cargo test -p geode-core --release --lib refined_capacitance
cargo test -p geode-core --release --test transmon_diffopt -- --include-ignored
```

The held-out series runs seven rounds by default
(`REFINED_OPT_HELD_OUT_ROUNDS`) and stops before a round above four million
tetrahedra (`REFINED_OPT_HELD_OUT_MAX_TETS`). `summary.toml` carries no timings
and is bit-reproducible; `results.json` differs between runs only in `solve_s`.
`tests/transmon_diffopt.rs` pins all four `summary.toml` files and re-derives
every bound, estimate and qubit-side number with its rounding direction; its release tier re-solves the
committed design on the 133k-tet fixture mesh.

Palace uses the existing local image `palace:cpu-1043` (source fba6a5b9d).
The runner records its immutable image ID, the input mesh name and SHA-256,
the commands (with the output directory relative to the repository root),
configurations and logs. Its resource limits are four CPUs and 12 GiB. AMR
intermediate-directory saving is disabled because Palace failed to copy files
on the Docker Desktop bind mount; that failed attempt is retained as
`palace_p2_amr_save_failure.log`. The exported meshes are reproducible and
ignored by Git: `final_coarse.msh` in a run directory is the committed coarse
mesh moved to the final design, and `adaptive/final_adapted.msh` is the adapted
optimization mesh with the same deformed polygonal surfaces.

## Which files are generated

| File | Origin |
|---|---|
| `*/summary.toml`, `*/results.json`, `*/design.json`, `palace_p{1,2,3}.json` next to a mesh | written by `transmon_refined_optimize` |
| `palace_validation*.json`, `palace_p*.log`, `palace_p*/`, `palace_p2_amr.json` | written by `scripts/transmon_refined_palace.py` and Palace |
| `validation_logs/*.log` | captured standard output of the commands above |
| `validation_summary.json` | **hand-assembled** index of the numbers above with their sources |
| `p2_cycles10/adaptive/failed_attempts.json` | **hand-assembled** record of the two failed Palace attempts |
| `cycles16/STOPPED.txt` | **hand-written** note on why that exploration stopped |

The adapted mesh was called `final_coarse.msh` when the Palace P3 attempt and
the failed AMR save attempt ran. Their retained logs and resolved
configurations (`palace_p3.log`, `palace_p3/`, `palace_p2_amr_save_failure*`)
still show that name. The file is the same: SHA-256 `dde0eac7...ddeb` before
and after the rename. The completed P2 and AMR runs were repeated under the new
name and reproduce the earlier capacitances to every digit.

## Exploratory runs retained

**Superseded (#1066): wrong anchor.** These targeted 89.9 fF.

These were written by an earlier version of the generator, which ran a single
held-out round, and were not regenerated. They have no `summary.toml`.

- `results.json`: P1 after eight local refinements, target reached on 265,774
  tets; one held-out round gave 88.081529220 fF (-2.02 %). Its independent
  Palace order sweep uses the exported coarse geometry.
- `cycles16/results.json`: P1 exploration stopped at the two-million-tet
  budget (14 accepted refinement cycles). It reached 89.900109291 fF and
  passed all final FD checks. The held-out solve was not completed; see
  `cycles16/STOPPED.txt`.

These explorations informed the choice of P2 and ten refinement cycles. That
choice was made after examining their results; it was not preregistered. Their
per-cycle P1 shifts decay by about 0.8 per cycle, which already indicated that
one refinement round cannot establish convergence.
