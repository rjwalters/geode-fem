# `geode eigen` — PEC-walled dielectric-sphere cavity

Two inputs on the same mesh: the lossless benchmark
(`sphere_cavity.json`) and a lossy, uniformly filled variant
(`lossy_sphere_cavity.json`, [below](#lossy-cavity-f-and-q)) whose `Q`
has an exact closed form.

## Lossless cavity

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

## Lossy cavity: f and Q

`lossy_sphere_cavity.json` (copy of
`tests/fixtures/sphere_lossy_pec_golden.json`, issue #706) fills the
**whole** cavity — `sphere_interior`, `vacuum_gap` and `pml_shell` (just a
region name: the `1.5 < r < 2` shell inside the PEC wall, no PML is
applied) — with one lossy dielectric `ε_r = 2.25 − 0.0225j`
(`tan δ = 0.01`). A lossy `eps_r` switches `geode eigen` to the complex
pencil (`solver.pencil = "complex_symmetric"`).

```sh
geode eigen crates/geode-cli/examples/eigen/lossy_sphere_cavity.json -o lossy_modes.json
geode eigen crates/geode-cli/examples/eigen/lossy_sphere_cavity.json --outdir lossy_modes/  # E_real + E_imag
```

### Expected output

`modes[]` ascending in frequency (`k0` / `lambda` = real parts, `k0_im` /
`lambda_im` = imaginary parts; `k0_im > 0` = decaying, `exp(+jωt)`):

| mode | `k0` | `k0_im` | `frequency_hz` | `q` |
|---|---|---|---|---|
| 0 | 0.915350 | 0.0045766 | 4.3675 GHz | 100.0025 |
| 1 | 0.915442 | 0.0045771 | 4.3679 GHz | 100.0025 |
| 2 | 0.915674 | 0.0045783 | 4.3690 GHz | 100.0025 |
| 3 | 1.292317 | 0.0064614 | 6.1661 GHz | 100.0025 |
| 4 | 1.293418 | 0.0064669 | 6.1713 GHz | 100.0025 |

With the whole cavity filled uniformly, the eigenvalue is `λ = μ₀/ε_r`
exactly, so every mode has `lambda_im / lambda = tan δ = 0.01` (to f64
round-off on the default CPU backend) and
`q = ½·cot(δ/2) = 100.0025` — the textbook `1/tan δ = 100` is only the
small-loss limit. The real parts are the lossless modes of the same mesh
at `ε′ = 2.25`, shifted by the loss (`|λ| = μ₀/|ε_r|`).
`tests/sphere_lossy_pec_golden.rs` holds both relations to `10⁻⁶` at
`tan δ = 0.01` and `0.1`.

## Order 2 at equal accuracy: PEC box cavity

`box_cavity_p1.json` and `box_cavity_p2.json` (issue #1265, Epic #836
Phase 5a) solve the same vacuum `2 × 0.9 × 1.0` PEC box (`guide_box_lc*.msh`,
cm units). The only differences are `"element_order"` and the mesh: p=1 on
the **finest** committed box mesh (`lc018`), p=2 on the **coarsest**
(`lc030`). A box has straight walls, so unlike the faceted sphere above
there is no geometry error and the analytic modes
`k = π·sqrt((m/a)² + (n/b)² + (p/d)²)` are an exact reference.

| spec | interior DOFs | wall time | peak RSS | k₀, lowest mode | error vs analytic 3.5124 |
|---|---|---|---|---|---|
| `box_cavity_p1.json` (`lc018`) | 1 662 | 0.25–0.40 s | 24 MB | 3.4956 | 0.48 % |
| `box_cavity_p2.json` (`lc030`) | 2 560 | 0.54–0.70 s | 41 MB | 3.5133 | 0.03 % |

p=2 on the coarsest mesh is more than ten times more accurate than p=1 on
the finest, for about 1.5–2× the DOFs and wall time. p=1 on the intermediate
`lc022` mesh gives 3.4952 (0.49 %), so mesh refinement alone does not close
the gap on these meshes. Raw reports and timings are in
[`benchmarks/p2_cookbook/`](../../../../benchmarks/p2_cookbook/); the timing
spread is host load (shared 8-core worker, measured 2026-11-06).
`tests/cookbook.rs` pins the ordering.

**Where p=2 does *not* pay off.** On the faceted `sphere.msh` the p=2 ground
mode is *less* accurate than p=1 on the same mesh (k₀ = 1.19737, 0.87 %
against the analytic 1.18710, versus 1.19145, 0.37 %), at 27× the wall time
and 19× the memory (21.8 s / 0.80 GB against 0.81 s / 0.04 GB). Straight-sided
tets approximate the sphere with the same facets at any order, so geometry error
dominates; a finer p=1 mesh (`sphere_fine.msh`, 0.02 %, 35 s) is the better
spend there. p=2 also costs far more on large driven problems: the spiral
smoke at `"element_order": 2` took 349 s and 7.8 GB against 3 s and 0.29 GB at
p=1, so it is not shipped as a cookbook example. Pick p=2 for smooth,
straight-walled geometry where the discretisation error dominates.
