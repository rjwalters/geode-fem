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
