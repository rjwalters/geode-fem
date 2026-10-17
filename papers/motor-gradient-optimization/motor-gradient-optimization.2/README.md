# Motor paper, revision 2

State: **REVISED; independent review and audit pending.** Authorship and venue remain open. This version incorporates the harmonic-coupling and scalar design-gradient studies. No optimization run or time-to-target speedup is reported.

## Build and check

From the version directory:

```sh
python3 verify.py
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v2.py
pdflatex -interaction=nonstopmode -halt-on-error main.tex
bibtex main
pdflatex -interaction=nonstopmode -halt-on-error main.tex
pdflatex -interaction=nonstopmode -halt-on-error main.tex
```

From the repository root, `papers/build.sh papers/motor-gradient-optimization/motor-gradient-optimization.2` also builds the PDF and reruns LaTeX until labels converge. The local class makes this version standalone. `main.bbl` is committed.

The renderer regenerates four manuscript figures, three generated manuscript tables (plus the sensitivity table in main.tex), and the carried-forward supplemental v1 plots/tables. `verify.py` is a read-only standard-library checker: it checks 78 file hashes, 1,640 forward solve statuses, paired coupling/estimator controls, sensitivity and cost tables, raw FD evaluations for every recorded case, citations and assets. It does not rerun FEM or replace independent scientific review.

## Evidence and limits

The manifest records each input's source path, commit and SHA-256. Original v1 inputs remain pinned to `581ee7ad40c9646f43e0d6df3de7b5a87d06f798`; new evidence is pinned to `910593bad6f64f9f01671c2632b57034e72b9c61`. Source snapshots are byte-preserved. Upstream reference data come from ICEMsnapshot `b145aaf4ab1a6424828bc941eaf172f48fa0d458`, GPL-3.0-or-later; the license is included. Solver and mesh regeneration commands live in the source benchmark READMEs, linked in the manuscript.

The new evidence comprises 1,140 harmonic forward solves and the gradient/parametrization records, in addition to the 500 original forward/sensitivity solves. The gradient records are a separate workload; 800 is the count of loaded objective–parameter comparisons, not the number of nonlinear solves.

The main conclusions concern this discrete benchmark. Cogging mismatch, the independent reference's discretization, straight-sided geometry and angular refinement remain unresolved. Timings used default threading on a loaded M3 Ultra; no controlled optimization timing is inferred. The grouped-control work in PR #1180 is not included in this evidence freeze.

See `changelog.md` for the response to the two v1 reviews, and `../EXPERIMENTS.md` for requested next measurements. V1 is unchanged. Citation resolver records and the primary-source metadata note support the added references; a formal citation audit remains pending.
