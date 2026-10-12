# Refined transmon optimization (#1056)

This experiment optimizes the **scalar-permittivity electrostatic capacitance**
of the DeviceLayout transmon, with a neutral floating feedline, toward the
89.9 fF anchor. The junction's 5.5 fF lumped self-capacitance is excluded. This
is not an optimization of the full-wave eigenfrequency or of the nonlinear
Hamiltonian.

## Result

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
  "Two-sided bound" below) puts it **above 88.78 fF**, at most 1.24 % below
  the anchor.
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

## Resolution-corrected design (an extrapolation target, not a certificate)

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
  places the converged value near it (see "Two-sided bound" below).
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

## Two-sided bound (#1062)

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
  is below 2e-7 of the island charge here.
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

| Geometry | Bracket (fF) | Width | Against the 89.9 fF anchor |
|---|---|---|---|
| Resolution-corrected design | **[89.84, 90.08]** | 0.24 fF (0.27 %) | contains it: −0.06 % to +0.20 % |
| Primary design | [88.78, 89.02] | 0.24 fF (0.28 %) | below it: 0.98 % to 1.24 % |
| X0 (adaptive and #1057 uniform meshes) | [77.40, 77.61] | 0.21 fF (0.28 %) | below it |

- **The answer for the corrected design.** Its converged capacitance is
  certified to lie in [89.84, 90.08] fF, which contains 89.9 fF. The bracket
  is 0.24 fF wide, so the corrected geometry matches the anchor to within
  −0.06 % / +0.20 % (from the unrounded bounds, rounded outward). The extrapolation band of the previous section
  (89.84–89.96 fF) lies inside it. This is a statement about the converged
  value of this polygonal geometry, not about the anchor's own accuracy or the
  junction self-capacitance.
- **The primary design** is certified 0.98–1.24 % below the anchor, which
  confirms the extrapolated 1.10–1.26 % but does not decide the 1 % threshold.
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

## Cross-solver check (same discretization)

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

From the repository root (about 20 min and up to 29.4 GB peak RSS for the first
command, about 10 min and 26.0 GB for the second; local, no cloud):

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
`tests/transmon_diffopt.rs` pins both `summary.toml` files and re-derives every
bound and estimate with its rounding direction; its release tier re-solves the
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
