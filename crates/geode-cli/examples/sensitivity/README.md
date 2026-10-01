# Material sensitivities — `∂C/∂ε_r`, `∂L_ij/∂ν_r` and `∂|S11|²/∂ε_r`

A `sensitivity` section (issues #707 / #739) is not an analysis of its
own: it rides on a `capacitance` (one terminal), `inductance`, lossless
`eigen` or one-lumped-port `driven` spec and adds a `sensitivities` block to that report — the exact discrete
gradient of the observable with respect to per-region material
parameters, from the library's FD-validated adjoint machinery. All three
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
* `driven_spiral.json` (copy of
  `tests/fixtures/driven_spiral_sensitivity_smoke.json`, issue #739): the
  3.5-turn spiral smoke (one 50 Ω lumped port, Leontovich copper, lossy
  `substrate` / `dielectric`) at 5 and 20 GHz; `|S11|²` and
  `∂|S11|²/∂ε_r` of both dielectrics at each frequency through the
  port-loaded adjoint (one LU per frequency serves the forward and the
  adjoint solve).

```sh
geode capacitance crates/geode-cli/examples/sensitivity/capacitance_coax.json -o c.json
geode inductance  crates/geode-cli/examples/sensitivity/inductance_triax.toml -o l.json
geode driven      crates/geode-cli/examples/sensitivity/driven_spiral.json -o s.json   # ~1.5 min debug
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
| spiral `\|S11\|²` | `substrate` `eps_r`, 5 GHz (`index [0]`) | 0.8974 | −2.749e-5 | 8.4e-8 (floored; 2.3e-6 plain) |
| spiral `\|S11\|²` | `dielectric` `eps_r`, 5 GHz | 0.8974 | −4.319e-4 | 2.5e-8 (1.3e-7 plain) |
| spiral `\|S11\|²` | `substrate` `eps_r`, 20 GHz (`index [1]`) | 0.9309 | −5.510e-5 | 7.6e-8 (1.1e-6 plain) |
| spiral `\|S11\|²` | `dielectric` `eps_r`, 20 GHz | 0.9309 | −1.329e-3 | 2.4e-8 (4.2e-8 plain) |

The capacitance observable is the **P2** two-terminal `C` (the library's
capacitance adjoint), reported in each entry's `value`; the report's
`c_farad` is the P1 extraction (81.90 fF here, the closed form is 81.66
fF). The driven entries' `value` is the report's own `|results[i].s[0][0]|²`;
their gradients sit below the `fd_rel_error` floor (`0.01·|value/p|`), so
the plain relative disagreement is quoted alongside. The two static
examples satisfy the Euler homogeneity identity an exact
gradient must: `Σ ε_k ∂C/∂ε_k = C` and `Σ ν_k ∂L_ij/∂ν_k = −L_ij` to
round-off (`tests/sensitivity_golden.rs`, which also covers the eigen
`∂f/∂ε_r` case on a synthetic cavity).

## Using it in an optimizer / agent loop

Edit the spec's `materials[].eps_r` (or `mu_r`), run, read
`sensitivities.entries[]`, step, repeat. Drop `fd_check` once a setup is
trusted: the gradient then costs one extra adjoint solve (capacitance),
one LU + forward + adjoint back-solve per frequency (driven), or nothing
beyond the forward solves (inductance, eigen).
