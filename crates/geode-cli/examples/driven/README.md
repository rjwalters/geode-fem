# `geode driven` — 3.5-turn spiral inductor

`spiral_inductor.json` (copy of `tests/fixtures/spiral_golden_smoke.json`)
is the spiral-inductor benchmark (issue #211) on its coarse smoke mesh
`spiral_3p5_smoke.msh` (µm units): a lossy Si substrate
(`ε_r = 11.9 (1 − j·0.005)`) and SiO₂ dielectric bound by physical-group
name, a PEC `outer_boundary`, Leontovich copper (`σ = 5.8e7 S/m`) on
`conductor_surface`, and one 50 Ω lumped port along +y whose width /
length are derived from the tagged faces. The sweep is 1 / 5 / 10 / 20 GHz
with the direct sparse LU.

```sh
geode check  crates/geode-cli/examples/driven/spiral_inductor.json
geode driven crates/geode-cli/examples/driven/spiral_inductor.json -o spiral.json
geode driven crates/geode-cli/examples/driven/spiral_inductor.json --touchstone spiral.s1p
```

## Expected output

`geode check`: 2 468 nodes, 10 733 tets, **13 997** interior edge DOFs
(`mesh.n_interior`), `analysis = "driven"`; the resource estimate is
order-of-magnitude only (~2 GB, deliberately conservative wall time at
this size).

`geode driven` — one `results[]` row per frequency with `z_ohm`, `y_s`,
`s` and the per-port `ports[0]` summary:

| f (GHz) | `l_h` (nH) | `r_ohm` (Ω) | `q` | `s_db` |
|---|---|---|---|---|
| 1  | 0.848 | 0.635 | 8.4  | −0.22 |
| 5  | 0.794 | 1.690 | 14.8 | −0.47 |
| 10 | 0.811 | 2.772 | 18.4 | −0.47 |
| 20 | 0.965 | 6.168 | 19.7 | −0.31 |

`tests/spiral_golden.rs` pins these to
`benchmarks/spiral_inductor/results_smoke.toml` (L within 1 %, R and Q
within 2 %, `|S11|` within 0.01). The smoke mesh is deliberately coarse;
the benchmark tier (`spiral_golden_benchmark.json`, `--ignored`) is the
one held to the Mohan / mom-PEEC oracle bands.

## Adaptive 40-point sweep (`sweep.adaptive`, issue #708)

`spiral_inductor_adaptive.json` is the same spiral over a 40-point
1–20 GHz band with an adaptive sweep:

```json
"sweep": { "adaptive": { "tolerance": 1e-6, "max_snapshots": 20 } }
```

```sh
geode driven crates/geode-cli/examples/driven/spiral_inductor_adaptive.json --progress -o spiral40.json
geode driven crates/geode-cli/examples/driven/spiral_inductor_adaptive.json --touchstone spiral40.s1p
```

Instead of 40 LU factorizations it spends **10** (the band ends and
midpoint, then 7 greedy picks), builds a 10-dimensional reduced model and
interpolates the other 30 frequencies through it. Expected
(`solver.adaptive`): `converged = true`, `n_factorizations = 10`,
`n_interpolated = 30`, `worst_residual ≈ 9e-8`; every row carries
`solved` (`true` for the 10 snapshot frequencies). Against the dense
40-point sweep (drop the `sweep` section) the worst `|ΔZ|/|Z|` is
4.0e-12 and the worst `|ΔS₁₁|` 3.7e-12, at 6.9 s vs 29.4 s wall (release,
M3 Ultra; issue #708). `--progress` streams one `snapshot` event per
full-order solve, then one `point` per frequency, on stderr. The dense
sweep can instead run frequencies in parallel: `--jobs 4` takes the same
40 points from 29.6 s to 6.5 s with bit-identical results (each in-flight
frequency holds its own LU factorization, so memory grows with `--jobs`).

## Rough copper (`roughness`, issue #758)

`spiral_inductor_rough.json` (copy of `tests/fixtures/spiral_rough_smoke.json`)
is the same spiral with **1 µm RMS** Hammerstad–Jensen roughness on the
copper:

```json
"leontovich": [{
  "physical_group": "conductor_surface",
  "conductivity_s_m": 5.8e7,
  "roughness": { "model": "hammerstad", "rms_m": 1e-06 }
}]
```

```sh
geode check  crates/geode-cli/examples/driven/spiral_inductor_rough.json   # leontovich[0].roughness.k
geode driven crates/geode-cli/examples/driven/spiral_inductor_rough.json -o spiral_rough.json
```

The wall impedance becomes `K(f)·(1 + j)·√(ωμ₀/2σ)` with
`K = 1 + (2/π)·atan(1.4·(Δ/δ)²)`; each `results[]` row reports the
`roughness_k` it applied. Expected (smooth values from the table above):

| f (GHz) | δ (µm) | `roughness_k[0].k` | `r_ohm` (Ω) smooth → rough | `l_h` (nH) smooth → rough |
|---|---|---|---|---|
| 1  | 2.09 | 1.197 | 0.635 → 0.738 | 0.848 → 0.869 |
| 5  | 0.93 | 1.645 | 1.690 → 2.537 | 0.794 → 0.830 |
| 10 | 0.66 | 1.807 | 2.772 → 4.517 | 0.811 → 0.847 |
| 20 | 0.47 | 1.901 | 6.168 → 10.672 | 0.965 → 1.010 |

The resistance rises by 78–82 % of `K − 1` rather than all of it, and `L`
by 2–5 %: the reactive half of `K·Z_s` (the conductor's internal
inductance) is scaled too, which shifts the current distribution on this
coarse mesh. `tests/roughness_golden.rs` pins each row to the smooth solve
at conductivity `σ/K(f)²` (the exact equivalent of a full-complex `K`) and
`K` to the closed form. For the Huray cannonball model use
`{"model": "huray", "ball_radius_m": 0.5e-6, "n_balls": 14, "tile_area_m2": 1e-10}`
(`K` = 1.047 / 1.143 / 1.206 / 1.278 at 1 / 5 / 10 / 20 GHz).

## Dispersive substrate (`dispersion`, issue #757)

`spiral_inductor_dispersive.json` (copy of
`tests/fixtures/spiral_dispersive_smoke.json`) replaces the substrate's
constant `ε_r = 11.9 (1 − j·0.005)` with a **Djordjevic–Sarkar** model
fitted to the same value at 1 GHz:

```json
{
  "physical_group": "substrate",
  "dispersion": {
    "model": "djordjevic_sarkar",
    "eps_r": 11.9, "tan_delta": 0.005, "f_ref_hz": 1e9
  }
}
```

```sh
geode check  crates/geode-cli/examples/driven/spiral_inductor_dispersive.json   # regions[].dispersion: eps_inf, delta_eps, eps_r(f)
geode driven crates/geode-cli/examples/driven/spiral_inductor_dispersive.json -o spiral_ds.json
```

Each frequency's operator is assembled with that frequency's `ε_r(f)`,
and each `results[]` row reports it in `materials[]`. Expected (constant
values from the table above):

| f (GHz) | `materials[0].eps_r` | tan δ | `l_h` (nH) constant → DS | `r_ohm` (Ω) constant → DS |
|---|---|---|---|---|
| 1  | [11.900, −0.05950] | 0.00500 | 0.8481 → 0.8481 | 0.6350 → 0.6350 |
| 5  | [11.839, −0.05935] | 0.00501 | 0.7943 → 0.7943 | 1.6899 → 1.6899 |
| 10 | [11.813, −0.05916] | 0.00501 | 0.8105 → 0.8105 | 2.7719 → 2.7715 |
| 20 | [11.786, −0.05878] | 0.00499 | 0.9650 → 0.9645 | 6.1676 → 6.1621 |

The 1 GHz row is the constant-ε fixture's row exactly (`f = f_ref`).
Silicon at `tan δ = 0.005` disperses little — `ε′` falls 1 % from 1 to
20 GHz, moving `L` by 0.05 % — so the change is small on this
conductor-dominated spiral; for an FR-4 laminate (`4.3`, `0.02`) `ε′`
falls ~3 % per decade. `tests/dispersive_golden.rs` pins every row to a
constant-ε run at that row's `ε_r(f)`.

## Debye and Drude substrates (issue #761)

Two more `dispersion` models on the same spiral (see the
[CLI README](../../README.md#models) for the formulas and rules):

* `spiral_inductor_debye.json` (copy of
  `tests/fixtures/spiral_debye_smoke.json`) — an illustrative **two-pole
  Debye** substrate, relaxations at ≈ 3 GHz and ≈ 30 GHz:

  ```json
  "dispersion": {
    "model": "debye", "eps_inf": 10.0,
    "poles": [{"delta_eps": 1.5, "tau_s": 5.3e-11}, {"delta_eps": 0.5, "tau_s": 5.3e-12}]
  }
  ```

* `spiral_inductor_drude.json` (copy of
  `tests/fixtures/spiral_drude_smoke.json`) — **Drude 10 Ω·cm n-type
  silicon**: lattice `ε∞ = 11.9`, `γ = e/(m*μ) = 5.0e12 rad/s`,
  `ω_p = √(σγ/ε₀) = 2.38e12 rad/s` for `σ = 10 S/m`. At GHz `ω ≪ γ`, so
  the carriers act as a conductor: `Im ε ≈ −σ/(ωε₀)`, and `Re ε` drops
  only by `ω_p²/γ² = 0.23`:

  ```json
  "dispersion": {
    "model": "drude", "eps_inf": 11.9, "omega_p_rad_s": 2.38e12, "gamma_rad_s": 5.0e12
  }
  ```

```sh
geode check  crates/geode-cli/examples/driven/spiral_inductor_drude.json   # regions[].dispersion: omega_p, gamma, eps_r(f)
geode driven crates/geode-cli/examples/driven/spiral_inductor_debye.json -o spiral_debye.json
geode driven crates/geode-cli/examples/driven/spiral_inductor_drude.json -o spiral_drude.json
```

Expected (`r_ohm`, `l_h` = `Re Z`, `Im Z/ω`; constant-ε values from the
table above):

| f (GHz) | Debye `eps_r` | Drude Si `eps_r` | `r_ohm` (Ω) constant / Debye / Drude | `l_h` (nH) constant / Debye / Drude |
|---|---|---|---|---|
| 1  | [11.850, −0.4663] | [11.673, −180.30] | 0.635 / 0.635 / 0.635 | 0.848 / 0.848 / 0.848 |
| 5  | [10.884, −0.7431] | [11.673, −36.06]  | 1.690 / 1.693 / 1.745 | 0.794 / 0.794 / 0.797 |
| 10 | [10.574, −0.5631] | [11.673, −18.03]  | 2.772 / 2.789 / 3.231 | 0.811 / 0.810 / 0.815 |
| 20 | [10.379, −0.4509] | [11.673, −9.009]  | 6.168 / 6.302 / 9.921 | 0.965 / 0.959 / 0.973 |

The Debye loss tangent peaks near the 3 GHz relaxation (0.039 → 0.068 →
0.053 → 0.043) while `ε′` falls 12 %; on this conductor-dominated spiral
that moves `R` by ≤ 2.2 %. The conductive silicon is the classic
spiral-on-CMOS-substrate loss: `R` at 20 GHz rises 61 % over the
`tan δ = 0.005` silicon. `tests/dispersive_golden.rs` pins every row of
both to a constant-ε run at that row's `ε_r(f)`, and the AMS solve of
both to direct LU (`--ignored`, release). A Drude **plasma** with
`Re ε < 0` (below `f₀ = re_eps_zero_hz`, echoed by `geode check`) solves
with `solver.mode = "direct"`; with `solver.preconditioner = "ams"` it is
`invalid_spec` (the AMS SPD proxy needs `Re ε > 0`).

## Waveguide into a lumped sheet (mixed ports, issue #759)

`waveguide_lumped_sheet.json` (copy of
`tests/fixtures/waveguide_mixed_smoke.json`) mixes both port kinds in one
spec. The mesh is a 2 cm × 1 cm × 1.2 cm air-filled rectangular guide
(`geode-core/tests/fixtures/waveguide_2x1_smoke.msh`, 8 × 4 × 4 cells,
PEC `walls`). `port_in` (`z = 0`) is a TE₁₀ **wave port**. `port_out`, the
whole `z = L` end face, is a **lumped port** across the 1 cm gap (`ê = ŷ`,
`width = 2`, `length = 1`). It acts as a uniform resistive sheet. Its
242.1 Ω matches TE₁₀ at `k₀ = 2.5` (11.93 GHz):
`R = η₀·(k₀/β)·(l/w)`.

```sh
geode check  crates/geode-cli/examples/driven/waveguide_lumped_sheet.json   # n_rhs_per_frequency = 3
geode driven crates/geode-cli/examples/driven/waveguide_lumped_sheet.json -o mixed.json
```

The S-matrix lists the lumped port first (row/column 0), then the wave
channel (1; `wave_ports[0].modes[0].channel = 1`). There is no impedance
matrix (`z_ohm` empty, `y_s` null). Expected (TE₁₀ cutoff 7.48 GHz):

| k₀ | f (GHz) | `S[0][0]` (sheet) | `S[1][0] = S[0][1]` | `S[1][1]` (wave) | `\|S_lw\|²` | `\|Γ\|` closed form |
|---|---|---|---|---|---|---|
| 2.0 | 9.54  | −0.091 + 0.056j | 0.075 − 0.885j | 0.114 + 0.019j | 0.789 | 0.112 |
| 2.5 | 11.93 | −0.165 + 0.063j | −0.618 − 0.647j | 0.009 − 0.008j | 0.800 | 0.001 |
| 3.0 | 14.31 | −0.192 + 0.082j | −0.891 − 0.062j | 0.046 + 0.004j | 0.798 | 0.046 |

`|S[1][1]|` follows the sheet mismatch `|Γ| = |(Z_s − Z_TE)/(Z_s + Z_TE)|`,
with `Z_TE = k₀/β`. `|S_lw|²` follows `8/π²·(1 − |Γ|²) ≈ 0.81`. The
lumped port's power wave reads only the *uniform* part of the sheet field
`E_y ∝ sin(πx/a)`. The rest of the TE₁₀ power is absorbed by the sheet
without reaching the port, so S is passive (`σ_max < 1`) but not unitary.
For the same reason `S[0][0] ≠ 0` at the match: the uniform drive also
excites evanescent TE₃₀, TE₅₀, …, which reflect reactively. `--touchstone`
is rejected for this spec, because a wave channel's reference impedance
`Z_TE(ω)` varies with frequency. `tests/wave_port_driven.rs` pins the
closed forms, reciprocity, passivity and library parity.
