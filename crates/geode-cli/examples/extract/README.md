# `geode extract` — SLCFET spiral, quasi-static `L₀`

`slcfet_spiral.json` (copy of `tests/fixtures/slcfet_extract_smoke.json`)
is the SLCFET 3HP spiral (issue #212) on its coarse smoke mesh
`spiral_slcfet_3hp_smoke.msh`: SiC substrate `ε_r = 9.7 (1 − j·0.004)`,
PEC `outer_boundary`, Leontovich Au on `conductor_surface`, one 50 Ω
lumped port. It shows the `extract` section's explicit **anchor ladder**:
`anchor_frequencies` (0.1 / 0.2 / 0.5 GHz, given in Hz) are solved in
addition to the 10 GHz sweep point; the two lowest anchors Richardson-
extrapolate `L₀ = lim_{f→0} Im Z / ω`, the third feeds the error estimate.

```sh
geode check   crates/geode-cli/examples/extract/slcfet_spiral.json
geode extract crates/geode-cli/examples/extract/slcfet_spiral.json -o l0.json
```

## Expected output

`geode check`: **13 630** interior edge DOFs, `analysis = "extract"`.

`geode extract` — four `results[]` rows, ascending over the union of the
sweep and the anchors, then per port `extraction[0]`:

| f (GHz) | `l_h` (nH) | `q` |
|---|---|---|
| 0.1 | 0.980 | 4.1 |
| 0.2 | 0.908 | 5.3 |
| 0.5 | 0.840 | 7.2 |
| 10  | 0.756 | 21.6 |

* `l0_h ≈ 1.004 nH` from the 0.1 / 0.2 GHz anchors;
* `l0_error_estimate_h ≈ 0.083 nH` (`l0_error_estimate_rel ≈ 8.3 %`, from
  the 0.5 GHz anchor) — large because this coarse smoke mesh is far from
  converged at the anchors. Add `"l0_rel_tol": 0.02` to the `extract`
  section to make such a run fail (`solve_failed`) instead of reporting;
* `srf_hz = null`: no `Im Z` sign change inside the sweep.

The smoke geometry differs from the benchmark (inner diameter 60 µm vs
100 µm), so no oracle `L₀` applies; `tests/slcfet_extract_golden.rs` holds
the CLI to library parity (1e-9) instead. The benchmark tier
(`slcfet_extract_benchmark.json`, `--ignored`) is held to the committed
`L₀` (1 %) and the mom PEEC oracle (5 %).

`extract` is **not** the static `geode inductance`: `l0_h` is an RF port's
`Im Z / ω` extrapolated to f → 0, including the port's own feed geometry.
