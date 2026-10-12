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

**Caveat: the anchor is a resonator-hybridized mode.** DeviceLayout's 4.14 GHz
is the lower of two coupled Palace eigenmodes (`[4.14, 5.591] GHz`, N = 2).
geode's `C_Sigma` treats the resonator as part of ground, so the pure-LC
inversion above reads a hybridized mode as if it were the bare qubit.
- **Sign.** Level repulsion from the 5.591 GHz mode pushes the Palace mode
  down. The bare-qubit-consistent `C_Sigma` target is therefore **below**
  93.9537 fF.
- **First-order size (the #1066 estimate; `g` is now measured, see
  [Against the bare-qubit target](#against-the-bare-qubit-target-1078)).** The shift is
  `g^2 (1/Delta + 1/Sigma)` with `Delta` = 1.451 GHz and `Sigma` = 9.731 GHz.
  `g / 2 pi` = 50, 80, 100 MHz gives 2.0, 5.1, 7.9 MHz, which is about 0.10,
  0.24, 0.38 fF in `C_Sigma`.
- **Ceiling.** #1043's same-mesh Palace-vs-LC offset is 0.281 %
  (3.452751 vs 3.462456 GHz). It mixes in solver and permittivity-model
  differences, so it is a ceiling of about 0.3 % in frequency, about 0.6 fF in
  `C_Sigma`, not a bound.
- **Consequence.** Every "relative to the anchor" figure below is measured
  against this ruled pure-LC anchor, not a hybridization-corrected one. Figures
  smaller than the possible offset are **not resolved by this anchor**: "at most
  0.20 % above", the -0.09 % / +0.08 % band, and "the band contains the
  anchor". The "at least 1.01 % below" figure survives, with its margin
  shrinking to about 0.4 % at the ceiling.
- Source spread: the Palace tutorial's own 4.0991 GHz would give about 95.95 fF.
- **Measured (#1078).** The coupling is now measured and the hybridization
  removed: the bare-qubit target is **93.853 fF**, band 93.826 to 93.892 fF
  (`TransmonAnchor::devicelayout_bare`). See
  [Against the bare-qubit target](#against-the-bare-qubit-target-1078). The
  dressed 93.9537 fF anchor above is kept, and every figure in this README
  that says "anchor" still means it.

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
  above the anchor, so the upper side alone places nothing near it. The
  #1075 certified lower bound adds the other side; see
  [Two-sided bound status](#two-sided-bound-status-1062-1075).
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
- an error estimator with a guaranteed lower bound;
- a substantially finer or higher-order solve.

The first is now supplied (#1075): #1062's equilibrated-flux lower bound,
applied on these same held-out meshes, certifies the converged value of this
geometry to lie in [93.89, 94.14] fF. See
[Two-sided bound status](#two-sided-bound-status-1062-1075), including what
that does and does not say about the hybridized anchor.

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

## Two-sided bound status (#1062, #1075)

The certified equilibrated-flux lower bound of #1062 has been applied to five
geometries. Each bracket is a rigorous statement about the **converged
electrostatic `C_Sigma` of that polygonal geometry** (scalar eps, feedline
floating, junction excluded), not about the physical device:

| Geometry | Bracket (fF) | Width | Artifact | Status |
|---|---|---|---|---|
| #1066 design (`p2_cycles10_anchor_93p95/`) | **[92.75, 93.00]** | 0.25 fF (0.27 %) | `../c_sigma_bracket_anchor_93p95.toml` | current anchor (#1075) |
| #1066 resolution-corrected design | **[93.89, 94.14]** | 0.25 fF (0.27 %) | `../c_sigma_bracket_anchor_93p95.toml` | current anchor (#1075) |
| X0 | [77.40, 77.61] | 0.21 fF (0.28 %) | `../c_sigma_bracket.toml` | anchor-independent, current |
| #1056 design (`p2_cycles10/`) | [88.78, 89.02] | 0.24 fF | `../c_sigma_bracket.toml` | **superseded** 89.9 fF target |
| #1058 resolution-corrected (`p2_cycles10_resolution_corrected/`) | [89.84, 90.08] | 0.24 fF | `../c_sigma_bracket.toml` | **superseded** 89.9 fF target |

The superseded-target brackets are described under
[Two-sided bound of the superseded designs](#two-sided-bound-of-the-superseded-designs-1062).
Whether they contain or miss 89.9 fF says nothing about the current target.

### The re-anchored designs (#1075)

`BRACKET_PART=anchor_93p95` runs the #1062 generator unchanged on the two
#1066 runs:
- it replays the ten P1 preparation cycles and the seven held-out rounds of
  each design;
- it asserts that every P2 value reproduces that run's `summary.toml`, so
  the meshes are the ones in the tables above;
- it certifies RT1, RT2 and Crouzeix–Raviart/RT0 lower bounds at every round,
  and RT3 at round 7.

X0 is not re-solved: the design increment is certified against X0's
committed bracket.

| Round | #1066 design: upper | best lower | width | Resolution-corrected: upper | best lower | width |
|---|---|---|---|---|---|---|
| 0 | 93.953668 | 92.123856 (RT2) | 1.829812 | 95.113017 | 93.236576 (RT2) | 1.876441 |
| 1 | 93.698451 | 92.294361 (RT2) | 1.404091 | 94.852767 | 93.412686 (RT2) | 1.440082 |
| 2 | 93.488380 | 92.421222 (RT2) | 1.067159 | 94.637932 | 93.543201 (RT2) | 1.094732 |
| 3 | 93.332106 | 92.516482 (RT2) | 0.815624 | 94.477743 | 93.642129 (RT2) | 0.835615 |
| 4 | 93.214011 | 92.590720 (RT2) | 0.623291 | 94.355733 | 93.719641 (RT2) | 0.636093 |
| 5 | 93.119308 | 92.649027 (RT2) | 0.470281 | 94.259623 | 93.778314 (RT2) | 0.481310 |
| 6 | 93.052518 | 92.689098 (RT2) | 0.363421 | 94.191269 | 93.820248 (RT2) | 0.371021 |
| 7 | 92.998561 | 92.758828 (RT3) | 0.239734 | 94.136104 | 93.891607 (RT3) | 0.244498 |

All values are in fF. Upper bounds are ceiled and lower bounds floored to 6
decimals. At round 7 the corrected design's lower bounds are RT1 93.748381,
RT2 93.856078, RT3 93.891607 and CR/RT0 92.946814 fF. The #1066 design's are
RT1 92.619465, RT2 92.724168, RT3 92.758828 and CR/RT0 91.837982 fF. Every
row is in the TOML.

**Anchor comparisons.** The anchor is 93.953660 fF, the pure-LC inversion of
the published 4.14 GHz. It is a resonator-hybridized mode (see
[The anchor](#the-anchor-1066)), so the bare-qubit-consistent target lies
**below** it:
- by an estimated 0.10 to 0.38 fF, giving a band of 93.574 to 93.854 fF
  (#1066's estimate, before `g` was measured; #1078 measures it, see
  [Against the bare-qubit target](#against-the-bare-qubit-target-1078));
- by at most about 0.6 fF at #1043's ceiling, giving 93.354 to 93.954 fF.
  The ceiling is not a bound.

What the brackets support, and what the anchor uncertainty prevents:
- **The #1066 design misses the target, certified.** Its converged
  capacitance lies 1.01 to 1.28 % below the anchor (from the unrounded
  bounds, rounded outward), at least 0.95 fF below it. That is more than the
  0.6 fF ceiling. The design stays below even the lowest plausible bare-qubit
  target, by at least 0.35 fF at the ceiling. This verdict survives the
  hybridization caveat. Optimizing to the target on the 448k-tet mesh
  overstates the converged value by at least 0.95 fF.
- **The resolution-corrected design contains the pure-LC anchor, certified.**
  Its bracket [93.89, 94.14] fF is −0.07 % to +0.20 % relative to 93.9537 fF. That is
  the first rigorous two-sided statement that the converged value of a design
  lies within 0.2 % of the current pure-LC anchor.
- **Whether it meets the physical target is not resolved** *(as of #1075;
  #1078 resolves it at its central coupling, see
  [Against the bare-qubit target](#against-the-bare-qubit-target-1078)).* The bracket
  contains the bare-qubit target only if the hybridization shift is below
  about 0.062 fF, the anchor minus the unrounded lower bound. The estimated
  shift is 0.10 to 0.38 fF. If that estimate holds, the converged value lies
  **above** the bare-qubit target: by at least 0.03 fF at a 0.10 fF shift,
  and by up to 0.57 fF at 0.38 fF. The bracket misses the estimated band
  (`meets_bare_qubit_estimate_band = false`). But `g` is not measured, and
  the bracket does overlap the ceiling band. A smaller shift remains
  possible, so no claim either way is made against the physical target.
- **The width is no longer the limiting factor.** The bracket is 0.25 fF
  wide. The estimated shift spans 0.28 fF (0.10 to 0.38), and its ceiling is
  about 0.6 fF. Tightening the bracket would not settle the anchor
  comparison; measuring `g`, or an anchor taken from a bare-qubit computation,
  would. #1078 does both.
- **The #1066 extrapolations against the certificate.** The free-rate fits
  and most of each geometric-tail band lie inside the brackets. The lowest
  geometric-tail values are excluded: 92.747491 fF for the #1066 design and
  93.875150 fF for the corrected design, both below the certified lower
  bounds. The slow-rate sensitivities are excluded outright:
  - #1066 design: 92.29 and 92.58 fF, against a lower bound of 92.75 fF;
  - corrected design: 93.41 and 93.71 fF, against a lower bound of 93.89 fF.

  The slow rates assumed there (alpha = 1/3 and 1/2) would put the limit
  below the certified lower bound, so they are ruled out for these series.

**Design increment over X0, certified.** Both designs and X0 are brackets of
converged values, so `C_design − C_X0` lies in
`[lower_design − upper_X0, upper_design − lower_X0]`. Here `upper_X0` is
77.609945 fF and `lower_X0` is 77.409607 fF (the unrounded family bounds of
`c_sigma_bracket.toml`). Rounded outward:

| Design | Certified increment over X0 (fF) |
|---|---|
| #1066 design | **[15.14, 15.59]** |
| #1066 resolution-corrected design | [16.28, 16.73] |

The #1066 design's held-out increment of 15.391 fF at round 7 lies inside
its interval.

**Resources.** The `anchor_93p95` part ran in 64.5 min with a peak RSS of
29.6 GB, locally (28 CPUs), no cloud. RT3 at round 7 took 9 to 10 min per
design. Standard output is in `validation_logs/anchor_93p95_bracket_run.log`.

**The `summary.toml` files predate #1075.** Their `bounds` text still says
there is no rigorous lower bound; `c_sigma_bracket_anchor_93p95.toml` and
this section supersede that line. They are generated and were not edited by
hand.

### Against the bare-qubit target (#1078)

#1078 measures the qubit-resonator coupling and removes the hybridization from
the anchor. Artifact: `../../transmon_coupling/results.toml` (generator
`examples/transmon_coupling_g.rs`; Palace cells in
`reference/fixtures/transmon_palace/results_coupling_1078/`).

**The coupling, measured.**
- **Sweep.** `L_J` takes 27 values from 4 to 30 nH on the committed
  133k-tet fixture, with the A2 port-voltage shunt. The bare qubit crosses the
  5.15 GHz lambda/4 resonator near 6.6 nH. The smallest sampled splitting is
  92.15 MHz.
- **Fit.** The model is two capacitively coupled LC nodes in Maxwell form,
  `(f_q^2 - f^2)(f_r^2 - f^2) = k^2 f^4`, with `f_q = a / sqrt(L_J + L_s)`.
  It fits all 54 branch frequencies to 6 kHz RMS (largest residual 28 kHz).
  The result is `k` = 0.017866, so `g / 2 pi` = 46.02 MHz at the crossing.
- **Model checks.**
  - The fitted circuit's eigenvectors match the solved junction
    participations to 5e-5.
  - A third node for the 3 lambda/4 harmonic (15.46 GHz) gets the same
    coupling (`k_3 / k` = 0.988), as an ideal lambda/4 line predicts.
  - Without `L_s`, a series loop inductance discussed below, the residuals
    reach 25 MHz.
- **Palace, same mesh.** At 6.40, 6.60 and 6.80 nH, the frequencies agree to
  2.5e-4 and `k` agrees to 3e-5 (0.017865).
- **Not mesh-converged.** On one uniform refinement (h/2), nine Palace cells
  give `k` = 0.022798 (`g / 2 pi` = 61.70 MHz), 28 % higher than on h.
- **Static proxy.** The resonator trace is carved out of the ground as its
  own electrode, and the coupling is computed as
  `k_s = C_g / sqrt((C_Sigma + C_J) C_rr / 2)` (tensor eps, P1). It rises
  from 0.01804 (h) to 0.02361 (h/2) to 0.02679 (h/4). The eigen `k` is 0.990
  of it on h and 0.965 of it on h/2.

**The anchor's coupling.** `k` rises at every measured level, so the anchor
uses an extrapolation, not a bound:
- four converged-`k` estimates span 0.0277 to 0.0299;
- the central value is `k` = 0.0288, with the band [0.0227, 0.0310];
- the band runs from the h/2 value to the static proxy's geometric-tail
  limit.

**The bare-qubit target.** The Maxwell-form inversion of DeviceLayout's pair
(4.14, 5.591) GHz gives the qubit frequency with the resonator held at 0 V.
That is the frequency `C_Sigma` describes, because `C_Sigma` grounds the
resonator.
- At `k` = 0.0288: `f_q` = 4.14209 GHz and **`C_Sigma` = 93.853 fF**,
  0.100 fF below the dressed anchor.
- Over the band, with the resonator's odd harmonics: **93.826 to 93.892 fF**,
  that is 0.062 to 0.128 fF below the anchor.
- **Transfer assumption.** One `k` describes DeviceLayout's own run, whose
  mesh is not geode's. The band uses this geometry's converged coupling. If
  DeviceLayout's 4.14 GHz came from a mesh as coarse as the fixture
  (`k` = 0.0179), the target would be 93.915 fF instead.

**Verdicts against the #1075 brackets.**

| Geometry | Certified bracket (fF) | Minus the target at `k` = 0.0288 (fF) | Over the whole band (fF) | Verdict |
|---|---|---|---|---|
| #1066 design | [92.75, 93.00] | -1.095 to -0.854 | -1.133 to -0.827 | **below** |
| #1066 resolution-corrected design | [93.89, 94.14] | +0.038 to +0.283 | +0.0002 to +0.310 | **above** |

The differences use the unrounded bounds and are rounded outward.
- **The resolution-corrected design's certified bracket does not contain the
  bare-qubit target.** At the central `k` its converged `C_Sigma` is 0.038 to
  0.283 fF above the target. Through the same linear model, its qubit would
  sit about 0.8 to 5.9 MHz below 4.14 GHz (an inference).
- **The margin at the band's low end is not resolved.** There, with `k` at
  its h/2 value, the bracket's lower bound clears the target by only
  0.0002 fF. "Above" therefore holds across the band only if the converged
  `k` exceeds the h/2 value. Every measured level points that way, but that is
  an inference.
- **The #1066 design stays below the target**, by at least 0.82 fF.
- **The two bare-frequency definitions differ here.** The issue's
  rotating-wave recipe, with this mesh's `g` = 46.02 MHz as an absolute
  frequency, gives 93.883 fF, also below the corrected design's lower bound.
  The Maxwell form at the same `k` gives 93.915 fF, inside it. The
  rotating-wave bare frequency is the Hamiltonian one, higher by
  `1/sqrt(1 - k^2)`, and is not what `C_Sigma` describes.

**Not applied: a series loop inductance.** The sweep also finds
`L_s` = 0.079 nH in the qubit mode, from the fit and, independently, from the
participations. That is about 0.5 % of its inductive energy outside the
junction (Palace on h/2: 0.093 nH).
- It predicts a 0.27 % downshift from the pure-LC frequency at 14.86 nH,
  which is most of #1043's 0.28 % offset.
- `TransmonAnchor` reads 4.14 GHz through `L_J` alone. Taking `L_s` into
  account would lower the target by roughly another 0.5 fF. This is an
  inference on unconverged meshes, and no anchor applies it; see
  `[loop_inductance]` in the artifact and the follow-up #1079.

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
  below the anchor (rounded down). The certified lower bound of #1062 (see
  "Two-sided bound of the superseded designs" below) puts it **above
  88.78 fF**, at most 1.24 % below the anchor.
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
  the band. That is why the band is an estimate and not a lower bound. Both
  slower rates are now ruled out: they lie below the certified 88.78 fF.
- **The 1 % working threshold of #1056.** After seven rounds the capacitance is
  0.985 % below its optimization-mesh value and still falling by 0.05 fF per
  round. The rigorous bound alone (at least 0.98 %) does not decide the
  threshold, and neither does the two-sided one (0.98 to 1.24 % below the
  anchor), which straddles 1 %. The estimate does: 1.10 to 1.26 % below the
  anchor. We do not claim the threshold is met, and we infer that it is not.
- **Sensitivities.** From round 0 to round 7 the three shape sensitivities move
  by -0.93 %, -0.67 % and -2.26 %. After one round they had moved by -0.18 %,
  -0.16 % and -0.60 %, which is what the first version of this study reported.
- **X0 and the design increment.** On the same meshes the undeformed geometry
  goes from 78.387918 to 77.609945 fF: its converged value is below 77.61 fF
  and above 77.40 fF (rigorous, #1062), estimated at 77.42 to 77.52 fF. This
  is inside #1057's
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
  above the anchor; the certified lower bound of #1062, 89.84 fF, is what
  places the converged value near the (superseded) 89.9 fF target (see
  "Two-sided bound of the superseded designs" below).
- **Estimate, not a bound.** The geometric tail gives 89.846092 to 89.951431 fF
  (midpoint 89.898762 fF) and the free-rate fits 89.956997, 89.926211 and
  89.952744 fF. The band is **89.84 to 89.96 fF**, within 0.07 % of the anchor
  on either side, and it contains the anchor.
- **Rate dependence.** At an assumed slower rate the same series extrapolates
  to 89.39 fF (alpha = 1/3) or 89.68 fF (alpha = 1/2), which is 0.57 % or
  0.25 % below the anchor. Both are below the certified 89.84 fF and ruled out.

This design is tuned so that an *extrapolation* lands on the anchor. The
extrapolation method was chosen on the first run and the second run uses it
unchanged, but both runs share the same mesh family, marking rule and element
order, so the agreement is not an independent confirmation.

**The band contains the anchor by construction** (#1062). The second run's
refinement series is the first run's series rescaled: the round-by-round ratio
of the two capacitances is constant to 8e-5, and the target ratio is 1.011988.
Any extrapolation of the second series is therefore the first run's
extrapolation times that ratio, and the 89.84–89.96 fF band contains the
89.9 fF anchor whatever extrapolation method is used. What the second run does
establish is:

- a rigorous upper bound: the corrected geometry converges below 90.08 fF;
- that the relative discretization bias transfers between the two nearby
  designs to about 1e-4.

The ±0.07 % band is the spread between methods, not an uncertainty. At the
slower rates the design lands 0.25–0.57 % below the anchor. The guaranteed
lower bound that the continuum statement needs is now measured separately; see
the next section.

### Two-sided bound of the superseded designs (#1062)

**Superseded target.** The two designs bracketed here are the #1056 / #1058
`p2_cycles10*` designs, aimed at the mis-derived 89.9 fF anchor (#1063,
#1066). The re-anchored `p2_cycles10_anchor_93p95*` designs are bracketed
separately (#1075); see
[Two-sided bound status](#two-sided-bound-status-1062-1075). The X0 bracket
does not depend on any anchor and remains current.

`../c_sigma_bracket.toml` (generator `examples/transmon_c_sigma_bracket.rs`,
library `assembly::electrostatic_dual`) adds a **certified lower bound** on the
same meshes. The generator replays the ten preparation cycles and the seven
held-out rounds of both runs, and asserts that every P2 value reproduces the
`summary.toml` of its run, so the meshes are the ones above.

- **Certificate.** For any piecewise-polynomial displacement field `D`,
  Thomson's principle and Cauchy–Schwarz give `C ≥ ε₀ N² / ∫ D·ε⁻¹D`. Here
  `N` is the island charge of `D` minus `|feedline charge|`, minus the `L¹`
  norms of the normal jumps across non-sheet faces, of the normal flux
  through the natural boundary and of `div D`. It uses `0 ≤ φ* ≤ 1` for the
  exact potential. The floating feedline enters as a charge constraint (zero
  net charge, free distribution), the dual of its free constant potential,
  so the bound is a bound on `C_ii − C_if²/C_ff` itself. Every term is
  integrated exactly. Solver tolerances only lower `N`; the penalty they cost
  is below 2e-7 of the island charge here. The arithmetic is plain `f64`, not
  interval arithmetic: the worst-case rounding error, about 1e-10 relative,
  is far below the 1e-8 per-mesh and 1e-4 per-family flooring.
- **Flux.** The P2 potential of each round is equilibrated patch by patch
  (Prager–Synge / Braess–Schöberl) into a Raviart–Thomas `RT_k` field,
  divergence-free and neutral on the feedline, on the mesh cut along the
  sheets. RT1 and RT2 are computed at every round and RT3 at round 7. A
  Crouzeix–Raviart solve (the lowest-order mixed RT0 dual, which needs no
  primal field) is the independent check.

| Round | Tets | Upper (fF) | RT1 | RT2 | RT3 | CR/RT0 | Width (fF) |
|---|---|---|---|---|---|---|---|
| 0 | 448,279 | 90.980104 | 88.647796 | 89.258866 | | 86.011746 | 1.721238 |
| 1 | 528,677 | 90.736603 | 88.947132 | 89.416769 | | 86.651461 | 1.319835 |
| 2 | 684,107 | 90.538648 | 89.172815 | 89.533730 | | 87.246482 | 1.004918 |
| 3 | 905,316 | 90.391038 | 89.341072 | 89.621776 | | 87.717193 | 0.769262 |
| 4 | 1,247,811 | 90.281357 | 89.472078 | 89.688994 | | 88.146917 | 0.592364 |
| 5 | 1,708,342 | 90.191606 | 89.580138 | 89.744988 | | 88.479278 | 0.446618 |
| 6 | 2,398,139 | 90.127900 | 89.652594 | 89.781381 | | 88.755432 | 0.346520 |
| 7 | 3,331,618 | 90.076692 | 89.717082 | 89.814751 | 89.847345 | 88.980016 | 0.229347 |

The table is the resolution-corrected design. Upper bounds are ceiled and
lower bounds floored to 6 decimals. Width is the upper bound minus the best
lower bound on that mesh.

Refinement does not move the polygonal surfaces, so every round bounds the
same converged value. Each family's bracket is its largest lower bound and its
smallest upper bound, rounded outward to 0.01 fF:

| Geometry | Bracket (fF) | Width | Against the superseded 89.9 fF target |
|---|---|---|---|
| Resolution-corrected design | **[89.84, 90.08]** | 0.24 fF (0.27 %) | contains it: −0.06 % to +0.20 % |
| Primary design | [88.78, 89.02] | 0.24 fF (0.28 %) | below it: 0.98 % to 1.24 % |
| X0 (adaptive and #1057 uniform meshes) | [77.40, 77.61] | 0.21 fF (0.28 %) | anchor-independent; current |

- **The corrected #1058 design.** Its converged capacitance is certified to
  lie in [89.84, 90.08] fF, 0.24 fF wide: −0.06 % / +0.20 % of the superseded
  89.9 fF target it was aimed at (from the unrounded bounds, rounded outward).
  The extrapolation band of the previous section (89.84–89.96 fF) lies inside
  it. This is a statement about the converged value of this polygonal
  geometry. It does not meet the current 93.9537 fF target, which this design
  was never aimed at.
- **The primary #1056 design** is certified 0.98–1.24 % below the superseded
  target, which confirms the extrapolated 1.10–1.26 % but does not decide the
  1 % threshold.
- **The design increment** over X0 is certified to lie in [11.17, 11.61] fF.
- **What limits the width.** At round 7 the bound rises by 0.10 fF from RT1 to
  RT2 and by 0.03 fF from RT2 to RT3, so the remaining width comes from the
  P2 field on these meshes, not from the flux order. The Crouzeix–Raviart bound converges like P1 and
  trails the equilibrated bounds by about 0.9 fF at round 7. At #1057's
  uniform levels the bracket is wide (X0 at P2 h/2: [74.05, 83.02] fF; the
  tensor-ε levels, P1 only: [72.96, 89.50] fF). The adapted meshes are what
  make it tight.
- **Resources.** The whole generator ran in 102 min with a peak RSS of 35.5 GB
  (the Crouzeix–Raviart solve on the 8.5M-tet uniform level), locally on
  28 CPUs shared with another job. RT3 at round 7 takes about 9 min.

The `summary.toml` files predate #1062. Their `bounds` text still says there
is no rigorous lower bound; this section and `c_sigma_bracket.toml` supersede
that line. They are generated and were not edited by hand.

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

The two-sided bounds (#1062, #1075), from the repository root, locally:

```sh
cargo run -p geode-core --release --example transmon_c_sigma_bracket
BRACKET_PART=anchor_93p95 \
cargo run -p geode-core --release --example transmon_c_sigma_bracket
```

The first writes `../c_sigma_bracket.toml` (X0 and the superseded designs,
102 min, 35.5 GB). The second writes `../c_sigma_bracket_anchor_93p95.toml`
(the #1066 designs, 64.5 min, 29.6 GB) and reads X0's bracket from the first.

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
