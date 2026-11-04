# Reproduce the v12 vector figures

Use Python 3.11 or newer with NumPy and Matplotlib (validated with Python 3.14,
NumPy 2.4.1 and Matplotlib 3.10.8). The figure verifier also needs Poppler
(`pdftotext`, `pdfinfo`); the manuscript build needs TeX Live. From the
repository root:

```sh
python3.14 papers/transmon-benchmark/transmon-benchmark.12/verify_figures.py
python3.14 papers/transmon-benchmark/transmon-benchmark.12/verify_evidence.py
papers/build.sh papers/transmon-benchmark/transmon-benchmark.12
```

`verify_figures.py` checks the input hashes in `data/provenance.json`, runs all
five scripts, requires byte-identical PDFs in the installed environment, and
checks the numbers each figure is built on against the committed artifacts.
Each `fig*.py` script can also be run on its own from any working directory.

## Files and printed numbers

| File | Printed | Section | Source script |
|---|---|---|---|
| `fig1-geometry.pdf` | Figure 1 | §3.2 | `fig1_geometry.py` (v9's, unchanged bytes) |
| `fig2-crosscheck.pdf` | Figure 2 | §5 | `fig2_crosscheck.py` (adds the second refinement family's estimate) |
| `fig3-initial-qubit.pdf` | Figure 3 | §5 | `fig3_initial_qubit.py` (new: both refinement families, the adaptive run) |
| `fig4-convergence.pdf` | Figure 4 | §6 | `fig4_convergence.py` (tensor bar at its true mesh size, inset on a fF scale) |
| `fig5-reoptimization.pdf` | Figure 5 | §7 | `fig5_reoptimization.py` (adds the scaling-law and Broyden controls) |

`figure_style.py` and `geometry_mesh.py` are v9's, unchanged. `save()` refuses a
figure that is too wide, has text below 8 pt at print size, has text off the
canvas, a legend over text, or an annotation over another text box.

## Data

Every plotted value is read from a committed artifact; there is no snapshot.
Figure 2 reads `benchmarks/transmon_devicelayout_crosscheck/results.toml` and
`benchmarks/transmon_devicelayout_controls/results.toml`; Figure 3 the same two;
Figure 4 the convergence study and the scalar and tensor bracket artifacts in
`benchmarks/transmon_diffopt/`; Figure 5
`benchmarks/transmon_devicelayout_optimize/results.toml`, its `opt_log.jsonl`
(the fidelity ratio of the last step, used only for the band-to-c map), the
controls artifact and the cross-check artifact. Estimates and bands are labelled as not bounds; no design
target from the 93.95 fF loop is drawn.
