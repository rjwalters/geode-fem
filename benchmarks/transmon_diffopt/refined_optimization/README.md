# Refined transmon optimization (#1056)

This experiment optimizes the **scalar-permittivity electrostatic capacitance**
of the DeviceLayout transmon, with a neutral floating feedline, toward 89.9 fF.
The junction's 5.5 fF lumped self-capacitance is excluded. This is not an
optimization of the full-wave eigenfrequency or of the nonlinear Hamiltonian.

The primary run is `p2_cycles10/results.json`. Ten P1 flux-jump refinement
cycles prepare a 448,279-tetrahedron mesh. The optimizer then uses **P2**
fields and exact solved shape derivatives, with AMG-PCG instead of sparse LU.
Its capacitance changes from 78.387918461 to 89.900518266 fF in three accepted
steps. The final geometry has island length 627.323864 um and pad width
29.087261 um (initially 625.5 and 24 um). The minimum volume ratio over
accepted steps is 0.61413; the final ratio is 0.61949. All three final
finite-difference checks pass; the largest relative discrepancy is 3.87e-7.

The committed-source repetition produced bit-identical numerical results
(excluding timings) and an identical exported-mesh SHA-256. See
`validation_summary.json` for provenance and checks.

## Validation

- A fresh P2 solve after an additional **local** refinement (528,324 tets)
  gives 89.662754721 fF, 0.264% below the target. This is empirical refinement
  stability, not a certified continuum error bound. The three shape
  sensitivities change by 0.177%, 0.160%, and 0.604%, respectively.
- Independent Palace P2 gives 89.900294115 fF, differing from GEODE by
  0.000249%. Palace adds 3,777 interior-boundary bisections: these are the
  same physical polygonal geometry and coefficients, not identical meshes.
- The attempted Palace P3 run terminated with signal 9 during flux-error
  estimation after its first terminal solve. It produced no complete
  capacitance matrix. Its log is retained; it is **not** validation evidence.
- Palace also completed two independent adaptive P2 steps (452,056 to
  475,079 elements). Its island effective capacitance remains 89.900294115 fF,
  while feedline self-capacitance changes by 1.90%. This suggests the marking
  concentrated on the feedline; we do not count this as a strong additional
  island-convergence check. See `p2_cycles10/adaptive/palace_validation_amr.json`.

No scalar-to-tensor equivalence, CAD regeneration, full-wave optimization,
experimental validation, global optimum, or speedup against Palace is claimed.
The geometry has zero-thickness sheet conductors and a natural zero-charge
outer boundary. Mesh refinement preserves its polygonal surfaces. The 1%
working validation threshold was set in issue #1056 before these runs.

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

Both unit tests check analytic scaling and independent finite differences;
the P1 test also compares AMG-PCG fields with LU. The real-device final checks
fully re-solve both perturbed geometries for every parameter, rather than
holding the potential fixed.

## Reproduction

From an isolated worktree root, with its private Cargo target:

```sh
CARGO_TARGET_DIR="$PWD/target" \
REFINED_OPT_ORDER=2 REFINED_OPT_CYCLES=10 REFINED_OPT_MAX_TETS=1000000 \
REFINED_OPT_DIR=benchmarks/transmon_diffopt/refined_optimization/p2_cycles10 \
cargo run -p geode-core --release --example transmon_refined_optimize

python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/adaptive --orders 2
python3 scripts/transmon_refined_palace.py \
  benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/adaptive \
  --orders 2 --amr-steps 2
cargo test -p geode-core --release --lib refined_capacitance
```

Palace uses the existing local image `palace:cpu-1043` (source fba6a5b9d).
The runner records its immutable image ID, input mesh SHA-256, complete
commands, configurations and logs (trailing blank lines normalized). Its resource limits are four CPUs and
12 GiB. AMR intermediate-directory saving is disabled because Palace
failed to copy files on the Docker Desktop bind mount; that failed attempt
is retained as `palace_p2_amr_save_failure.log`. The successful AMR run
retains its complete log and final matrix. No cloud is used. The exported meshes are reproducible and ignored
by Git. Despite the legacy basename `final_coarse.msh`, the file in the
`adaptive/` directory is the adapted mesh; its parent is the original coarse
mesh with precisely the same deformed polygonal surfaces.

## Exploratory runs retained

- `results.json`: P1 after eight local refinements, target reached on 265,774
  tets but **failed** held-out refinement (88.081529220 fF, -2.02%). Its
  independent Palace order sweep uses the exported coarse geometry.
- `cycles16/results.json`: P1 exploration stopped at the two-million-tet
  budget (14 accepted refinement cycles). It reached 89.900109291 fF and
  passed all final FD checks. The held-out solve was not completed; see
  `cycles16/STOPPED.txt`. It is not a completed refinement validation.

These explorations informed the choice of P2 and ten refinement cycles. That
choice was made after examining their results; it was not preregistered.
