# Numerical audit — conformal-antenna-diffopt.4

Audited 2026-07-21 against the three committed artifacts of record on `main`:

- A = `benchmarks/patch_antenna_conformal/conformal_results.toml`
- B = `benchmarks/fdtd_density_baseline/staircasing_results.json`
- C = `benchmarks/fdtd_density_baseline/meep_runtime_scaling.json`

v4 froze all numbers from the audited v3; this pass re-verifies text vs
artifacts. **62 numerical claims checked, 0 discrepancies.**

## Headline design result (abstract, §4, §7, Fig. captions) vs A

| Text claim | Artifact value | Match |
|---|---|---|
| 73 active freeform boundary DOFs | `n_freeform_dofs = 73`, `shape.n_dofs = 73` | yes |
| worst-of-band −5.51 → −12.06 dB | `worst_s11_db_initial = -5.506281`, `_final = -1.206262e1` | yes |
| FD rel. error 1.17×10⁻¹⁰ | `rel_err = 1.166263e-10` | yes (3 s.f.) |
| analytic = central dir. deriv. = 1.751288204×10⁻¹ | `analytic_dir_deriv = central_fd_dir_deriv = 1.751288204e-1` | yes |
| FD step 10⁻⁶, tolerance 5×10⁻³ | `fd_step_h = 1.0e-6`, `tolerance = 5.0e-3` | yes |
| n_fact = 1 per band frequency | `n_factorizations_per_freq = 1` | yes |
| 6 accepted steps of a 600-step cap, terminal `target_reached`, 0 backtracks | `n_steps = 6`, `max_steps = 600`, `terminal_condition = "target_reached"`, `total_backtracks = 0` | yes |
| objective 1.897943102×10⁻¹ → 3.411688781×10⁻², factor 5.56 | `band_objective_initial/final`, `reduction_factor = 5.563060` | yes |
| per-freq dB: −12.06 / −23.92 / −14.42 at ω = 0.30/0.35/0.40 | `s11_db = -1.206262e1 / -2.392241e1 / -1.442428e1` | yes |
| per-freq mag: 0.2494 / 0.0637 / 0.1900 | `s11_mag = 2.493841e-1 / 6.366189e-2 / 1.900142e-1` | yes |
| worst vol ratio 0.572 vs 0.25 budget | `worst_vol_ratio = 5.721208e-1`, `min_vol_ratio_budget = 0.25` | yes |
| max nodal displacement 0.560 | `max_nodal_disp_mm = 5.597611e-1` | yes |
| grad norm 1.751×10⁻¹ → 8.987×10⁻² | `grad_norm_initial/final = 1.751288204e-1 / 8.986832799e-2` | yes |
| fresh forward reproduces objective to relative 0.0 | `fresh_vs_optimizer_rel = 0.000000e0` | yes |
| fixture: 6173 edges, 997 nodes, 4875 tets | `n_edges/n_nodes/n_tets` | yes |
| bend radius 40, PML shell 8, port R = 50, band {0.30, 0.35, 0.40}, −10 dB target | `bend_radius_mm = 40`, `pml_thick_mm = 8`, `port_resistance_ohm = 50`, `band_omega`, `target_db = -10` | yes |

Note (non-critical): §4 "the run is bit-identical on re-run" is not a field of
A; carried verbatim from the AUDITED v3. Not contradicted by any artifact.

## Geometric-fidelity axis (§5.1) vs B

| Text claim | Artifact value | Match |
|---|---|---|
| conductor faces on radii 40.8 / 39.2, φmax = 0.3 rad, ~24.1 feature | `R_top_mm = 40.8`, `R_bot_mm = 39.2`, `phi_max_rad = 0.3`, `feature_width_mm = 24.114` | yes |
| N=20: cell 1.206, perim 13.0%, area 13.58%, RMS 0.291 | 1.2057 / 0.13036 / 0.13576 / 0.29098 | yes |
| N=40: cell 0.603, perim 15.4%, area 0.33%, RMS 0.157 | 0.60286 / 0.15391 / 0.00325 / 0.15675 | yes |
| N=80: cell 0.301, perim 14.2%, area 0.33%, RMS 0.095 | 0.30143 / 0.14214 / 0.00325 / 0.09478 | yes |
| N=160: cell 0.151, perim 14.2%, area 0.15%, RMS 0.045 | 0.15072 / 0.14214 / 0.00148 / 0.04509 | yes |
| area slope ≈1.96, boundary slope ≈0.88, perimeter slope −0.026 (flat) | 1.9552 / 0.8796 / −0.02594 | yes |
| perimeter plateau ~+14% | 0.14214 at N=80/160 | yes |
| ~6029 cells across feature, ~250 cells/mm, ~1 μm target | `cells_across_feature_needed = 6028.61`, 6028.6/24.114 = 250.0, `conformal_target_mm = 0.001` | yes |
| ~5.35×10⁴× the finest-grid Yee cells (3-D) | `equivalent_3d_fdtd_cell_blowup_factor = 53492.4` | yes |
| tet reference boundary RMS = 0 to machine precision | `conformal_reference.boundary_pos_rms_mm = 0.0` | yes |

## Compute axis (§5.2, §5.3) vs C

| Text claim | Artifact value | Match |
|---|---|---|
| R=4: 10.3 M cells, 0.233 s/step, 1.6 GB, dt 0.125 | 10321920 / 0.233343 / 1.573 / 0.125 | yes |
| R=6: 34.8 M, 0.742, 4.9 GB, 0.0833 | 34836480 / 0.742044 / 4.918 / 0.083333 | yes |
| R=8: 82.6 M, 1.715, 11.3 GB, 0.0625 | 82575360 / 1.714985 / 11.338 / 0.0625 | yes |
| R=10: 161.3 M, 3.386, 21.8 GB, 0.05 | 161280000 / 3.386214 / 21.836 / 0.05 | yes |
| R=12: 278.7 M, 5.761, 37.4 GB, 0.0417 | 278691840 / 5.761447 / 37.417 / 0.041667 | yes |
| cells = 161280·R³ exactly | `cells_vs_R = "exactly 161280 * R^3"` | yes |
| s/step ~R^2.92, RAM ~R^2.89, dt ∝ 1/R (0.125→0.0417), solve ~R^3.92 ≈ R⁴ | `scaling_law` block, verbatim | yes |
| host: AWS m6i.4xlarge, 16 vCPU Ice Lake, 61 GB; Meep 1.34.0 single-process; vendored `reference/meep/docker/`; 61 steady-state steps after warmup | `provenance` block, verbatim | yes |
| anchor: R=8 ran ≥421 steps, no decay in 15-min window, ≥12 min/forward, ≥24 min/gradient | `measured_anchor` + `single_gradient_min_at_R8 = ">=24"` | yes |
| projections: ≥14 h at R=8, ≥70 h at R=12 (3 freqs, ≥12 evals) | `full_band_optimization_hours_at_R8/R12 = ">=14" / ">=70"` | yes |
| RAM ~60 GB at R=14, ~83 GB at R=16, R≥14 does not fit on 61 GB box | `R14_peak_rss_gb = 60`, `R16_peak_rss_gb = 83`, `R16_runnable_on_61GB_box = false`; artifact conclusion: "RAM wall at R>=14 on a 61 GB box" | yes |
| ~2.5×10¹⁵ cells at curve-faithful ~250 cells/mm (abstract/§5.3 head-to-head round to ~10¹⁵); ~30× beyond RAM wall | `cells_at_curve_faithful_resolution = "~2.5e15...~30x the RAM wall"` | yes |
| §5.2/Fig. caption: R=12 still staircases (~14% perimeter error) | inference from B's flat perimeter plateau (0.14214, slope −0.026); R=12 cells/mm ↔ cell 0.083 mm, finer than B's finest tested (0.151 mm) on a flat error curve | yes (labeled inference, consistent) |

## Text-internal consistency

- Abstract, §5.3 head-to-head, and §7 repeat the headline numbers (−12.06 dB,
  ~+14%, ~R³/R⁴, 61 GB, R≥14, ~10¹⁵ cells) verbatim-consistently — the known
  deliberate repetition, frozen from v3. No internal contradiction found.
- Projections are labeled as projections at every occurrence (abstract, §5.2,
  §5.3, §6, §7), matching the artifact's own `projection.disclaimer`.

## Figure source-of-truth check (informational)

`figures/` (3 rendered figures + `src/`) is **byte-identical to
`.3/figures/`** (md5-verified, all 5 rendered files). Source scripts
(`plot_runtime_scaling.py`, `plot_s11_band.py`, `setup_schematic.tex`) share
the same mtime as the rendered outputs (copied together); no source script is
newer than its render. **No stale-figure signal.**
