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
