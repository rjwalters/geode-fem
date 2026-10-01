# Material sensitivities — `∂C/∂ε_r` and `∂L_ij/∂ν_r`

A `sensitivity` section (issue #707) is not an analysis of its own: it
rides on a `capacitance` (one terminal), `inductance` or lossless `eigen`
spec and adds a `sensitivities` block to that report — the exact discrete
gradient of the observable with respect to per-region material
parameters, from the library's FD-validated adjoint machinery. Both
examples also set `"fd_check": {}`, so the binary re-solves the forward
problem at `p·(1 ± 1e-4)` per parameter and fails the run if any gradient
disagrees with the central difference by more than `1e-4`.

* `capacitance_coax.json` (copy of
  `tests/fixtures/capacitance_coax_sensitivity_smoke.json`): the coax
  capacitance smoke mesh (`a = 1`, `m = 1.6`, `b = 2.5` mm, `L = 1` mm),
  `ε_r = 2` in `dielectric_inner`, one terminal `inner` against ground
  `outer`; gradients w.r.t. both annuli's `eps_r`.
* `inductance_triax.toml` (copy of
  `tests/fixtures/inductance_triax_sensitivity_smoke.toml`): the triax
  inductance smoke mesh with a `μ_r = 2` core; the full 2×2 `L` matrix
  differentiated w.r.t. the relative reluctivity `nu_r = 1/μ_r` of `core`,
  `tube` and `gaps`.

```sh
geode capacitance crates/geode-cli/examples/sensitivity/capacitance_coax.json -o c.json
geode inductance  crates/geode-cli/examples/sensitivity/inductance_triax.toml -o l.json
jq '.sensitivities.entries[] | {parameter, index, gradient, fd_rel_error}' c.json
```

## Expected output

`sensitivities.entries[]` has one entry per (parameter, observable
component); `gradient` is in observable units per unit parameter (the
parameters are dimensionless):

| spec | entry | `value` | `gradient` | FD rel. error |
|---|---|---|---|---|
| coax C | `dielectric_inner` `eps_r` | 81.35 fF (P2) | 14.06 fF | 4.7e-9 |
| coax C | `dielectric_outer` `eps_r` | 81.35 fF | 53.22 fF | 1.5e-9 |
| triax L | `core` `nu_r`, `L[core][core]` | 0.3174 nH | −0.1955 nH | 9.8e-9 |
| triax L | `gaps` `nu_r`, `L[core][tube]` | 0.0960 nH | −0.0620 nH | 1.0e-8 |

The capacitance observable is the **P2** two-terminal `C` (the library's
capacitance adjoint), reported in each entry's `value`; the report's
`c_farad` is the P1 extraction (81.90 fF here, the closed form is 81.66
fF). Both examples satisfy the Euler homogeneity identity an exact
gradient must: `Σ ε_k ∂C/∂ε_k = C` and `Σ ν_k ∂L_ij/∂ν_k = −L_ij` to
round-off (`tests/sensitivity_golden.rs`, which also covers the eigen
`∂f/∂ε_r` case on a synthetic cavity).

## Using it in an optimizer / agent loop

Edit the spec's `materials[].eps_r` (or `mu_r`), run, read
`sensitivities.entries[]`, step, repeat. Drop `fd_check` once a setup is
trusted: the gradient then costs one extra adjoint solve (capacitance) or
nothing beyond the forward solves (inductance, eigen).
