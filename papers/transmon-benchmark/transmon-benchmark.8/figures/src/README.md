# Reproduce the v8 vector figures

Use Python 3.11 or newer with NumPy and Matplotlib (validated with Python 3.14,
NumPy 2.4.1 and Matplotlib 3.10.8). The figure verifier also needs Poppler
(`pdftotext`); the manuscript build needs TeX Live. From the repository root:

```sh
python3.14 papers/transmon-benchmark/transmon-benchmark.8/verify_figures.py
python3.14 papers/transmon-benchmark/transmon-benchmark.8/verify_evidence.py
papers/build.sh papers/transmon-benchmark/transmon-benchmark.8
```

`verify_figures.py` checks the input hashes in `data/provenance.json`, runs all
eight scripts, requires byte-identical PDFs in the installed environment, and
checks the numbers each figure is built on against the committed artifacts.
Each `fig*.py` script can also be run on its own from any working directory.
PDFs are vector output with embedded fonts and no creation timestamps. Other
Matplotlib or font versions may change the bytes without changing the data.

## Files and printed numbers

In v8 the file number is the printed figure number.

| File | Printed | Section | Source script |
|---|---|---|---|
| `fig1-geometry.pdf` | Figure 1 | §4.1 | `fig1_geometry.py` |
| `fig2-agreement.pdf` | Figure 2 | §5 | `fig2_agreement.py` |
| `fig3-lscaling.pdf` | Figure 3 | §5 | `fig3_l_scaling.py` |
| `fig4-participation.pdf` | Figure 4 | §6.3 | `fig4_participation.py` |
| `fig5-convergence.pdf` | Figure 5 | §7 | `fig5_convergence.py` |
| `fig6-refined-optimization.pdf` | Figure 6 | §9.4 | `fig6_refined_optimization.py` |
| `fig7-cpu-wallclock.pdf` | Figure 7 | §11.1 | `fig7_cpu_wallclock.py` |
| `fig8-legacy-diffopt.pdf` | Figure 8 | Appendix A | `fig8_legacy_diffopt.py` |

v7 used different file names for four of these (`fig6-participation`,
`fig7-convergence`, `fig5-diffopt`, `fig4-cpu-wallclock`). Panel (b) of the v7
convergence figure, the held-out series, is panel (b) of Figure 6 here.

## Style and layout rules

`figure_style.py` is self-contained; no `.anvil/` payload is needed. Every
figure is drawn at most 6.3 inches wide and included at 0.98 of the 6.5-inch
text width, so it is never shrunk and no font is below 8 pt on the page.
`save()` refuses to write a figure that is too wide, has text below 8 pt, has
text off the canvas, has a legend over any text, or has an annotation over
another annotation, tick label, axis label or title. Those checks look at text
boxes only; text over a data line is caught by looking at the rendered page.

Colors: blue is the qubit mode or the primary optimized design; green the
resolution-corrected design or the scalar bound; teal the cavity modes or the
feedline; gray the port-sheet artifact, the initial geometry or historical
data; orange the junction or the 89.9 fF anchor. Marker shape and labels
distinguish series as well. Stiffness participation and port EPR never share an
axis; the timing comparison uses one linear axis.

## Data

Every plotted data value is read from a committed artifact except one: the
optimized island outline of Figure 6, `data/optimized_island.json`. The run's
exported meshes are gitignored by the artifact directory, so the outline is a
snapshot; `data/provenance.json` records what it is, the SHA-256 of the mesh it
came from, and the consistency check against the committed `design.json`.
Figure 6 reads the two committed `summary.toml` files for everything else. It
shows the rigorous upper bounds, the estimate bands (not bounds) and the
slower-rate fits, and labels the corrected design's band as containing the
anchor by construction.
