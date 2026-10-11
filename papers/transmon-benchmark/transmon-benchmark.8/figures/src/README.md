# Reproduce the v8 vector figures

Use Python 3.11 or newer with NumPy and Matplotlib (validated with Python 3.14
and Matplotlib 3.10.8). The verifier also requires Poppler (`pdftotext`);
the manuscript build requires TeX Live. From the repository root:

```sh
python3.14 papers/transmon-benchmark/transmon-benchmark.8/verify_figures.py
python3.14 papers/transmon-benchmark/transmon-benchmark.8/verify_evidence.py
papers/build.sh papers/transmon-benchmark/transmon-benchmark.8
```

The figure verifier runs all seven source scripts, checks source hashes and
selected numerical assertions, and requires byte-identical PDF regeneration
in the installed environment. Individual `fig*.py` scripts can also be run
directly from any working directory. `figure_style.py` is self-contained;
no private Anvil style package is needed. PDFs are vector outputs with embedded
fonts and omitted creation timestamps. Cross-version Matplotlib/font changes
may alter PDF bytes even when the plotted data are unchanged.

File names retain the v7 numbering where useful: `fig6-participation.pdf` is
printed Figure 4, `fig4-cpu-wallclock.pdf` is printed Figure 6. The new
`fig5-diffopt.pdf` shows refined optimization; the former experiment is
`fig7-legacy-diffopt.pdf` in the appendix.

Blue marks the physical qubit or optimized design; teal marks independent
validation or the feedline; gray marks the port-sheet artifact, baseline,
or historical data; orange marks the junction or optimization target. Marker
shape and text also distinguish the series. Participation observables have
separate axes; timing comparisons share one linear scale. The new optimization
band is a working tolerance, not statistical uncertainty.

See `data/provenance.json` and `../../provenance.md` for pinned inputs and limits.
The compact Figure 5 snapshots let this draft render without checking out
PR #1058. Other figure scripts require the committed repository benchmark
files. The manuscript PDF can be built standalone from its vendored class,
BBL, TeX, bibliography, and already-rendered figures.
