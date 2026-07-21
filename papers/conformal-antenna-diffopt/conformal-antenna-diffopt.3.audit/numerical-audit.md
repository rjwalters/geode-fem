# Numerical audit — conformal-antenna-diffopt.3

**Context:** v3 is a cosmetic-only polish of the AUDITED-clean v2. A `diff` of `main.tex` v2→v3 confirms the ONLY changes are prose: header comment, "intractable" wording scoped to single-process/single-node Meep 1.34.0 on a 61 GB host (4 spots), placeholder-caption strip (3 captions), and two redundant `-12.06` dB parenthetical trims (intro contribution list + conclusion; the number is retained in abstract, §4 head-to-head, and §5.3 Results table). **No numeric literal was added, removed, or altered.** Every value below re-traces to the on-disk artifacts, confirming 0 discrepancies and no regression vs v2.

**Sources of truth:**
- H = `benchmarks/patch_antenna_conformal/conformal_results.toml` (headline design)
- S = `benchmarks/fdtd_density_baseline/staircasing_results.json` (geometric-fidelity axis)
- M = `benchmarks/fdtd_density_baseline/meep_runtime_scaling.json` (compute axis)

## Headline design + method (source H)

| Text claim | Source | Source value | Match | Notes |
|---|---|---|---|---|
| 73 active freeform boundary DOFs | H | n_freeform_dofs=73 | yes | |
| 6173 edges, 997 nodes, 4875 tets | H | n_edges/n_nodes/n_tets | yes | |
| bend radius 40, box-UPML 8, port R=50 | H | bend_radius_mm=40, pml_thick_mm=8, port_resistance_ohm=50 | yes | read as natural units per §Method |
| band ω∈{0.30,0.35,0.40} | H | band_omega=[0.3,0.35,0.4] | yes | |
| FD rel err 1.17×10⁻¹⁰ | H | rel_err=1.166263e-10 | yes | rounds to 1.17e-10 |
| analytic = central dir. deriv 1.751288204×10⁻¹ | H | analytic/central_fd_dir_deriv=1.751288204e-1 | yes | |
| FD step 10⁻⁶, tol 5×10⁻³ | H | fd_step_h=1.0e-6, tolerance=5.0e-3 | yes | |
| 6 accepted steps, 600-step cap, target_reached | H | n_steps=6, max_steps=600, terminal_condition=target_reached | yes | |
| 0 total backtracks | H | total_backtracks=0 | yes | |
| objective 1.897943102×10⁻¹ → 3.411688781×10⁻² | H | band_objective_initial/final | yes | |
| reduction factor 5.56 | H | reduction_factor=5.563060e0 | yes | |
| worst-of-band −5.51 → −12.06 dB | H | worst_s11_db_initial=-5.506281, final=-1.206262e1 | yes | |
| grad norm 1.751×10⁻¹ → 8.987×10⁻² | H | grad_norm_initial=1.751288204e-1, final=8.986832799e-2 | yes | |
| worst vol ratio 0.572 vs 0.25 budget | H | worst_vol_ratio=5.721208e-1, min_vol_ratio_budget=0.25 | yes | |
| max nodal displacement 0.560 | H | max_nodal_disp_mm=5.597611e-1 | yes | rounds to 0.560 |
| fresh forward reproduces to relative 0.0 | H | fresh_vs_optimizer_rel=0.000000e0 | yes | |
| S11 table ω=0.30: −12.06 dB / 0.2494 mag | H | s11_db=-1.206262e1, s11_mag=2.493841209e-1 | yes | |
| S11 table ω=0.35: −23.92 dB / 0.0637 mag | H | s11_db=-2.392241e1, s11_mag=6.366188709e-2 | yes | |
| S11 table ω=0.40: −14.42 dB / 0.1900 mag | H | s11_db=-1.442428e1, s11_mag=1.900141779e-1 | yes | |
| n_factorizations = 1 per band frequency | H | n_factorizations_per_freq=1 | yes | |

## Geometric-fidelity axis (source S)

| Text claim | Source | Source value | Match | Notes |
|---|---|---|---|---|
| bend radius 40; radii 40.8 / 39.2; φ_max=0.3 rad; ~24.1 wide | S | R_bend=40, R_top=40.8, R_bot=39.2, phi_max_rad=0.3, feature_width=24.1144 | yes | |
| N=20: cell 1.206, perim 13.0%, area 13.58%, RMS 0.291 | S | 1.2057, 0.13036, 0.13576, 0.29098 | yes | |
| N=40: cell 0.603, perim 15.4%, area 0.33%, RMS 0.157 | S | 0.60286, 0.15391, 0.00325, 0.15675 | yes | |
| N=80: cell 0.301, perim 14.2%, area 0.33%, RMS 0.095 | S | 0.30143, 0.14214, 0.00325, 0.09478 | yes | |
| N=160: cell 0.151, perim 14.2%, area 0.15%, RMS 0.045 | S | 0.15072, (perim 0.14214), 0.00148, 0.04509 | yes | |
| perimeter plateaus at ~+14% | S | perimeter_rel_err ~0.14 flat | yes | |
| boundary-RMS = 0 for conformal tet at any resolution | S | conformal_reference.boundary_pos_rms_mm=0.0 | yes | |
| ~6029 cells across feature, ~250 cells/mm | S | cells_across_feature_needed=6028.6 (6029/24.1≈250) | yes | |
| ~1 µm target fidelity | S | conformal_target_mm=0.001 | yes | |
| ~5.35×10⁴ × Yee cells of finest grid | S | equivalent_3d_fdtd_cell_blowup_factor=53492 | yes | |

## Compute axis (source M)

| Text claim | Source | Source value | Match | Notes |
|---|---|---|---|---|
| Meep 1.34.0, m6i.4xlarge, 16 vCPU, 61 GB, 61 steps | M | meep_version 1.34.0; host m6i.4xlarge 16 vCPU 61 GB; 61 timesteps | yes | |
| R=4: 10.3M cells, 0.233 s/step, 1.6 GB, dt 0.125 | M | 10321920, 0.233343, 1.573, 0.125 | yes | |
| R=6: 34.8M, 0.742, 4.9 GB, 0.0833 | M | 34836480, 0.742044, 4.918, 0.083333 | yes | |
| R=8: 82.6M, 1.715, 11.3 GB, 0.0625 | M | 82575360, 1.714985, 11.338, 0.0625 | yes | |
| R=10: 161.3M, 3.386, 21.8 GB, 0.05 | M | 161280000, 3.386214, 21.836, 0.05 | yes | |
| R=12: 278.7M, 5.761, 37.4 GB, 0.0417 | M | 278691840, 5.761447, 37.417, 0.041667 | yes | |
| cells = 161280·R³ | M | scaling_law.cells_vs_R | yes | |
| s/step ~R^2.92, RAM ~R^2.89, dt∝1/R (0.125→0.0417) | M | scaling_law | yes | |
| single forward ~R^3.92 ≈ R⁴ | M | single_forward_solve_wallclock_vs_R | yes | |
| R=8 anchor: ≥421 steps, ≥12 min, no decay in 15 min; ≥24 min adjoint | M | measured_anchor + single_gradient_min_at_R8=">=24" | yes | |
| ≥14 h at R=8, ≥70 h at R=12 (band opt) | M | full_band_optimization_hours_at_R8/R12 | yes | |
| peak RAM ~60 GB at R=14, ~83 GB at R=16; R≥14 exceeds 61 GB | M | R14_peak_rss_gb=60, R16_peak_rss_gb=83, R16_runnable=false | yes | |
| ~2.5×10¹⁵ cells at curve-faithful resolution; ~30× RAM wall | M | cells_at_curve_faithful_resolution ~2.5e15 | yes | |

## Figure source-of-truth (informational)

`figures/src/` exists. All 3 rendered figures (`setup_schematic.pdf`, `s11_band.pdf`, `runtime_scaling.pdf`) were carried forward unchanged from `.2` and embed correctly (log: `<./figures/setup_schematic.pdf>` p5, `<./figures/s11_band.pdf>` p6, `<./figures/runtime_scaling.pdf>` p8). Not re-checking mtime staleness: v3 is caption-text-only; the figure renders were not regenerated and the reviser did not intend them to be. No stale-figure note raised.

**Result:** 0 numerical discrepancies. 0 numerical inconsistencies. No regression vs the AUDITED-clean v2 — every value re-traces to the committed artifacts.
