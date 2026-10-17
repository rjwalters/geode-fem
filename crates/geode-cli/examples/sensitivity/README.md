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

## N-port: `∂S`, `∂Z₀`, `∂ε_eff` w.r.t. a strip width (issue #883)

* `microstrip_strip_width.json`: the 50 Ω shielded microstrip cookbook
  line (`driven/microstrip_line.json`, hybrid wave ports, `w/h = 1.91` on
  `ε_r = 4.4`) at 2 GHz, with two parameters — the strip width (a
  `shape` parameter: the `strip` group stretched along x, its value the
  width 1.91 mm, the `shield` pinned; the port faces move in their plane
  and the hybrid port modes follow them) and the substrate `eps_r` — and
  four observables: the input port's `z0` and `eps_eff`, `S11` in dB and
  `∠S21` in degrees.

```sh
geode driven crates/geode-cli/examples/sensitivity/microstrip_strip_width.json \
    --check-gradient -o ms.json      # ~1 min release (the graded face)
jq '.sensitivities.entries[] | {parameter, observable, gradient, fd_rel_error}' ms.json
```

`--check-gradient` re-runs the shipped forward with the strip moved by
`±1e-4·w` (the mesh morphed, never re-meshed) and the substrate's `ε_r`
moved by `±1e-4·ε_r`:

| parameter | observable | `value` | `gradient` | FD rel. error | Hammerstad–Jensen (open line) |
|---|---|---|---|---|---|
| `strip_width` (m) | `z0` (Ω) | 49.27 | −15 297 Ω/m | 1.7e-8 | −15 744 Ω/m |
| `strip_width` | `eps_eff` | 3.328 | 147.3 /m | 3.2e-7 | 149.3 /m |
| `strip_width` | `db(S[0,0])` | −68.43 dB | 2973 dB/m | 2.8e-6 | |
| `strip_width` | `phase_deg(S[1,0])` | −8.787° | −181.7 °/m | 5.7e-8 | |
| `substrate` `eps_r` | `z0` | 49.27 | −5.041 Ω | 3.6e-8 | −5.118 Ω |
| `substrate` `eps_r` | `eps_eff` | 3.328 | 0.6842 | 3.0e-8 | 0.6805 |

The closed-form column is the differentiated Hammerstad–Jensen formula of
the **open** line; the shielded `20h × 12h` box and the mesh put geode
within 3 %.

`db(S[0,0])` is not a physical sensitivity here: at −68 dB the matched
line's `S11` is the mesh's residual reflection, so its gradient (exact for
the morphed, fixed-topology mesh — the FD check passes) measures how that
discretization residue moves; a re-meshed central difference gives
−4.96 dB/mm against the adjoint's +2.97 dB/mm. Read `∂S11` only where `S11`
is well above the mesh's reflection floor.

Drop `"pinned": ["shield"]` and the run warns
(`sensitivities.warnings[]`, kind `sensitivity_shape_moves_boundary`) that
the stretch also widens the PEC shield box; turn the stretch into a
`translate` without `pinned` and it warns (`sensitivity_shape_rigid`) that
the whole model translates rigidly — every gradient then zero by
construction.

**The N-port FD floor** (issue #890). A driven entry's `fd_rel_error` is
`|g − g_FD| / max(|g|, |g_FD|, floor)`, with `floor` the largest of

* `0.01 · gmax`, where `gmax` is the largest `|gradient|` of the same
  parameter and observable over the frequencies;
* `0.01 · M / L`: 1 % of the observable's natural gradient scale. `M` is
  `max(|value|, 1)`, or `20/ln 10` for dB and `180/π` for degrees. `L` is
  the parameter's natural scale: `|ε′|`, or the motion length for a shape
  parameter;
* `ρ / (h · tol)`: the FD estimate's own round-off `ρ/h`, divided by the
  tolerance. Here `ρ` is the forward's round-off of the value: `2e-12`
  absolute on an S entry and `1e-11` relative on `z0` / `ε_eff`, mapped
  through the form (dB and phase divide it by `|z|`). The one-sided loss
  difference uses `4ρ/h`.

The second and third terms do not shrink with an entry that is zero by
symmetry. Without them, shifting the strip laterally (`translate` x,
`shield` pinned) or rigidly moving the whole model fails `--check-gradient`
on mesh-asymmetry or round-off noise. With them, both now pass.
`ρ` is about 10× the round-off measured on this cookbook: with a rigid
translation, every FD difference is pure round-off, and at relative steps
`1e-3` to `1e-5` that round-off was ≤ 1.6e-13 on `S11` and `S21`,
≤ 1.2e-12 on `ε_eff` and ≤ 5.8e-13 on `z0`.

The floor is not loose enough to hide a real adjoint error. A 1 % error is
caught on any entry above `1e-4 · M/L` at the default
`tol = step = 1e-4`. For example, the lateral shift's `∂z0` is
−15.08 Ω/m, 6e-4 of its natural scale, and a 1 % error on it gives
`rel = 6.2e-4`. A scratch mutation that scaled parameter 0's gradients by 1.01 made five
`--check-gradient` tests in `tests/sensitivity_nport.rs` fail with
`fd_check_failed`. On the lateral shift's `∂ dB(S11)` the failure was at
rel 4.5e-3.

## Using it in an optimizer / agent loop

Edit the spec's `materials[].eps_r` (or `mu_r`), run, read
`sensitivities.entries[]`, step, repeat. Drop `fd_check` once a setup is
trusted: the gradient then costs one extra adjoint solve (capacitance),
one LU + forward + adjoint back-solve per frequency (driven), or nothing
beyond the forward solves (inductance, eigen).
