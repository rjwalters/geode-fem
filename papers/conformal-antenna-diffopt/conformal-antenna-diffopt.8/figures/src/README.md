# Reproduce the conformal-antenna-diffopt.8 figures

Python 3.11 or newer with NumPy and Matplotlib (rendered with Python 3.14.8,
NumPy 2.4.1 and Matplotlib 3.10.8). No solver run is needed: every script reads
committed artifacts under `benchmarks/patch_antenna_conformal/`.

```
cd papers/conformal-antenna-diffopt/conformal-antenna-diffopt.8/figures/src
for s in fig1_geometry fig2_accuracy fig3_optimizers fig4_response; do python3.14 -I $s.py; done
```

| Figure | Script | Sources |
|---|---|---|
| 1 geometry | `fig1_geometry.py` | `baseline_2g4_L1.toml` `[meta]`; `recentre/geometry/rbG_L1_copper_38_sm57_w16.json` |
| 2 forward accuracy | `fig2_accuracy.py` | `v6/v6_summary.toml` `[x1]` (`[[x1.geode_p1]]`, `[x1.palace.init_L1_p*]`, `[x1.rim_fit]`); `convergence.toml` `[[s11.bent]]` |
| 3 optimizers | `fig3_optimizers.py` | `copper/L1_copper_38.toml`; `designc/c4/L1_copper_38_fdgrad.toml`; `v6/x4/L1_copper_38_cma*.toml`; `tuned/L1_copper_38_{cma1000_sig15_s11,fcma190_sig05_s11,newuoa190_npt40}.toml`; `v6/palace_loop/L1_copper_38_pnewuoa190.toml`; `tuned/tuned_summary.toml` (solves-to-level and `[[scaling]]`, which the curves and points must reproduce); `designc/designc_summary.toml [c4]` |
| 4 response | `fig4_response.py` | `recentre/palace/sm57_p{2,3}/port-S.csv`; `designc/c3/c3_sm57_rim020_p2/port-S.csv`; `recentre/rbG_L1_copper_38_sm57_w16_on_{rbGcomb_L1_cyl_dense,rbGcomb_L1_cyl_edge_hi,rbGrim020_L1_cyl_dense,rbG_L1_dense}.toml`; `designc/designc_summary.toml [c3]` (the 1.9 MHz margin the curve must reproduce); `tuned/tuned_summary.toml [[air_margin]]` |

No data value is hard-coded. `fig3_optimizers.py` imports the counting functions
of `benchmarks/patch_antenna_conformal/scripts/tuned_summary.py`
(`adjoint_curve`, `gf_curve`) and asserts that the drawn curves give the
summary's solves-to-level for every run. `fig4_response.py` (b) asserts that the
0.2 mm-rim curve crosses -10 dB at the summary's lower-edge margin. The scripts add
their own directory to `sys.path`, so they run under `python3 -I`. Figure 1(c) now
draws the final width as a dimension line.

`figure_style.py` draws every figure at most 6.3 in wide (included at 0.98 of
the 6.5 in text width, never shrunk) and refuses to save a figure with text
under 8 pt, text off the canvas, or overlapping labels. Set `FIG_DRAFT=1` to
keep a failing render for inspection.
