# `geode eigen` — PEC-walled dielectric-sphere cavity

`sphere_cavity.json` (copy of `tests/fixtures/sphere_pec_golden.json`) is
the dielectric-sphere cavity benchmark (issues #26 / #69) on
`sphere.msh` (cm units): a sphere of `ε_r = 2.25` (`n = 1.5`) on the
`sphere_interior` group, every other region vacuum by default, and a PEC
`outer_boundary` at `r = 2`. No ports and no frequencies — the `eigen`
section asks for the 5 modes nearest the shift `k₀ = 1` (natural units,
rad per mesh unit), just below the lowest mode.

```sh
geode check crates/geode-cli/examples/eigen/sphere_cavity.json
geode eigen crates/geode-cli/examples/eigen/sphere_cavity.json -o modes.json
geode eigen crates/geode-cli/examples/eigen/sphere_cavity.json --outdir modes/   # + one .vtu per mode
```

## Expected output

`geode check`: **3 300** interior edge DOFs (the pencil dimension),
`analysis = "eigen"`.

`geode eigen` — `modes[]` ascending in frequency:

| mode | `k0` | `frequency_hz` |
|---|---|---|
| 0 | 1.1914 | 5.685 GHz |
| 1 | 1.1918 | 5.687 GHz |
| 2 | 1.1919 | 5.687 GHz |
| 3 | 1.8088 | 8.631 GHz |
| 4 | 1.8104 | 8.638 GHz |

Modes 0–2 are the (discretisation-split) triplet of the lowest spherical
cavity mode. Every mode has `q = null` — the pencil is lossless, so `Q` is
undefined, not a number — and a tiny `residual_rel` (~1e-13).
`solver.n_null_filtered` counts the curl-curl gradient-nullspace Ritz
values (`k₀ ≈ 0`) the Lanczos run discarded. `tests/sphere_pec_golden.rs`
pairs each mode to an analytic PEC-cavity Mie root within 15 % on `k`
(coarse mesh) and, in its `--ignored` tier, to a dense QZ solve of the same
pencil to 1e-6.
