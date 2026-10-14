# Reproduce the conformal-antenna-diffopt.5 figures

Python 3.11 or newer with NumPy and Matplotlib (validated with Python 3.14.5,
NumPy 2.5.4 and Matplotlib 3.10.8). No solver run is needed: every script reads
committed artifacts under `benchmarks/patch_antenna_conformal/`.

```
cd papers/conformal-antenna-diffopt/conformal-antenna-diffopt.5/figures/src
uv run --python 3.14 --with matplotlib==3.10.8 --with numpy python fig1_geometry.py
uv run --python 3.14 --with matplotlib==3.10.8 --with numpy python fig3_response.py
uv run --python 3.14 --with matplotlib==3.10.8 --with numpy python fig2_convergence.py
```

| Figure | Script | Sources |
|---|---|---|
| 1 geometry | `fig1_geometry.py` | `baseline_2g4_L1.toml` `[meta]`; `geometry/rbD_L1_copper_38_sm45.json` |
| 2 convergence | `fig2_convergence.py` | `convergence.toml` `[[s11.bent]]`, `[gate_g1.bent]` |
| 3 response | `fig3_response.py` | `copper/verify/rbD_L1_copper_38_sm45_on_rbD_{L1_p2,L3_cyl}_dense*of3.toml`, `convergence/split/dip_L1_p2_bent.toml`, `nested_dof/verify/initial_on_L1_p2.toml`, `copper/verify/L1_copper_38_on_L1{_dense0of1,_p2}.toml`, `copper/g2_summary.toml`, `convergence.toml` |

The only hard-coded data value is the fitted 47.85 MHz
shift in `fig3_response.py` (from `scripts/copper_shift_fit.py`, recorded in
`copper/README.md`).

`figure_style.py` draws every figure at most 6.3 in wide (included at 0.98 of
the 6.5 in text width, never shrunk) and refuses to save a figure with text
under 8 pt, text off the canvas, or overlapping labels. Set `FIG_DRAFT=1` to
keep a failing render for inspection.
