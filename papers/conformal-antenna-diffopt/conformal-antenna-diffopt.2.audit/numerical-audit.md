# Numerical audit — conformal-antenna-diffopt.2

Every load-bearing DATA number in `main.tex` was checked directly against the committed on-disk artifacts (not merely text-vs-figure). **0 numerical inconsistencies.** All three figures render (PDF present) and are not stale (source scripts older than or equal to the rendered PDFs).

## A. Headline design — `benchmarks/patch_antenna_conformal/conformal_results.toml`

| Text claim | Location | Source value (toml) | Match |
|-----------|----------|---------------------|-------|
| worst-of-band −5.51 → −12.06 dB | abstract, §Results, §Concl | worst_s11_db_initial −5.506281; final −1.206262e1 | ✓ |
| FD rel err 1.17×10⁻¹⁰ | abstract, §Results, §Concl | rel_err 1.166263e-10 | ✓ |
| analytic = central dir deriv 1.751288204×10⁻¹ | §Results | analytic/central_fd_dir_deriv 1.751288204e-1 | ✓ |
| FD step 10⁻⁶, tol 5×10⁻³ | §Results | fd_step_h 1.0e-6; tolerance 5.0e-3 | ✓ |
| 73 active freeform boundary DOFs | throughout | n_freeform_dofs 73; shape.n_dofs 73 | ✓ |
| 6173 edges, 997 nodes, 4875 tets | §Results Fixture | n_edges 6173; n_nodes 997; n_tets 4875 | ✓ |
| bend radius 40, box-UPML 8, port R=50 | §Results, §Method | bend_radius_mm 40; pml_thick_mm 8; port_resistance_ohm 50 | ✓ |
| band ω∈{0.30,0.35,0.40}, target −10 dB | §Method, §Results | band_omega [0.3,0.35,0.4]; target_db −10 | ✓ |
| 6 accepted steps of 600-step cap, target_reached | §Results | n_steps 6; max_steps 600; terminal_condition target_reached | ✓ |
| 0 total backtracks | §Results | total_backtracks 0 | ✓ |
| objective 1.897943102×10⁻¹ → 3.411688781×10⁻², factor 5.56 | §Results | band_objective_initial/final match; reduction_factor 5.563060 | ✓ |
| grad norm 1.751×10⁻¹ → 8.987×10⁻² | §Results | grad_norm_initial 1.751288204e-1; final 8.986832799e-2 | ✓ |
| worst vol ratio 0.572 vs 0.25 budget | §Results | worst_vol_ratio 5.721208e-1; min_vol_ratio_budget 0.25 | ✓ |
| max nodal displacement 0.560 | §Results | max_nodal_disp_mm 5.597611e-1 | ✓ |
| fresh forward reproduces to relative 0.0 | §Results | fresh_vs_optimizer_rel 0.000000e0 | ✓ |
| per-freq |S11| dB: −12.06 / −23.92 / −14.42 | §Results table + fig | s11_band points −1.206262e1 / −2.392241e1 / −1.442428e1 | ✓ |
| per-freq |S11| mag: 0.2494 / 0.0637 / 0.1900 | §Results table | 2.493841209e-1 / 6.366188709e-2 / 1.900141779e-1 | ✓ |

## B. Geometric-fidelity axis — `benchmarks/fdtd_density_baseline/staircasing_results.json`

| Text claim | Source value (json) | Match |
|-----------|---------------------|-------|
| N=20 table row (cell 1.206, perim 13.0%, area 13.58%, RMS 0.291) | cell_size 1.2057; perim 0.1303648; area 0.1357552; rms 0.2909806 | ✓ |
| N=40 (0.603, 15.4%, 0.33%, 0.157) | 0.6028612; 0.1539141; 0.0032504; 0.1567495 | ✓ |
| N=80 (0.301, 14.2%, 0.33%, 0.095) | 0.3014306; 0.1421394; 0.0032504; 0.0947814 | ✓ |
| N=160 (0.151, 14.2%, 0.15%, 0.045) | 0.1507153; 0.1421394; 0.0014819; 0.0450888 | ✓ |
| area slope ≈1.96, boundary slope ≈0.88, perimeter slope −0.026 (flat) | area 1.9552192; boundary 0.8796031; perimeter −0.0259439 | ✓ |
| perimeter plateaus at ~+14% | rows plateau 14.2% at N≥80 | ✓ |
| ~6029 cells across ~24.1 feature = ~250 cells/mm | cells_across_feature_needed 6028.612; feature_width 24.1144 (6029/24.11≈250) | ✓ |
| ~5.35×10⁴× the finest-grid Yee cells | equivalent_3d_fdtd_cell_blowup_factor 53492.40 | ✓ |
| arc geometry: radii 40.8 / 39.2, φ_max 0.3 rad | R_top 40.8; R_bot 39.2; phi_max_rad 0.3 | ✓ |
| conformal boundary RMS 0 to machine precision | conformal_reference.boundary_pos_rms_mm 0.0 | ✓ |

## C. Compute axis — `benchmarks/fdtd_density_baseline/meep_runtime_scaling.json`

| Text claim | Source value (json) | Match |
|-----------|---------------------|-------|
| R=4 (10.3M cells, 0.233 s/step, 1.6 GB, dt 0.125) | cells 10321920; 0.233343; 1.573; 0.125 | ✓ |
| R=6 (34.8M, 0.742, 4.9 GB, 0.0833) | 34836480; 0.742044; 4.918; 0.083333 | ✓ |
| R=8 (82.6M, 1.715, 11.3 GB, 0.0625) | 82575360; 1.714985; 11.338; 0.0625 | ✓ |
| R=10 (161.3M, 3.386, 21.8 GB, 0.05) | 161280000; 3.386214; 21.836; 0.05 | ✓ |
| R=12 (278.7M, 5.761, 37.4 GB, 0.0417) | 278691840; 5.761447; 37.417; 0.041667 | ✓ |
| cells exactly 161280·R³ | scaling_law cells_vs_R "exactly 161280 * R^3" | ✓ |
| s/step ~R^2.92, peak RAM ~R^2.89, dt~1/R (0.125→0.0417), forward ~R^3.92≈R⁴ | scaling_law exponents 2.92 / 2.89 / 1/R / R^3.92 | ✓ |
| R=8 anchor: ≥421 steps, ≥12 min/forward, ≥24 min/adjoint | measured_anchor steps ">=421", wallclock ">=12"; projection single_gradient ">=24" | ✓ |
| ≥14 h band opt at R=8, ≥70 h at R=12 | full_band_optimization_hours_at_R8 ">=14"; _at_R12 ">=70" | ✓ |
| peak RAM ~60 GB at R=14, ~83 GB at R=16; R≥14 exceeds 61 GB box | R14_peak_rss_gb 60; R16_peak_rss_gb 83; R16_runnable false | ✓ |
| ~250 cells/mm ⇒ ~2.5×10¹⁵ cells, ~30× RAM wall | resolution ~250; cells "~2.5e15 ... ~30x the RAM wall" | ✓ |
| Meep 1.34.0, m6i.4xlarge, 16 vCPU, 61 GB, 61 steps | provenance meep_version 1.34.0; host m6i.4xlarge 16 vCPU 61 GB; note "over 61 timesteps" | ✓ |

## Figure source-of-truth (informational)

| Figure | Rendered | Source | Stale? |
|--------|----------|--------|--------|
| figures/s11_band.pdf | present | figures/src/plot_s11_band.py | no (src not newer) |
| figures/runtime_scaling.pdf | present | figures/src/plot_runtime_scaling.py | no |
| figures/setup_schematic.pdf | present | figures/src/setup_schematic.tex | no |

All three figures render into the 10-page PDF. No stale figures.

## Note (non-critical, reviewer item (a))

Three figure captions still carry stale "(Placeholder — rendered by paper-figures ...)" parenthetical text (3 occurrences confirmed in the rendered PDF via pdftotext) even though the figures now render correctly. Cosmetic; recommend stripping. Does NOT break the build and is not a numerical inconsistency.
