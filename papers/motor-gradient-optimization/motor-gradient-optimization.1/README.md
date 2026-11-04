# Motor paper: first manuscript

State: **DRAFTED**, unreviewed and unaudited. Authorship and venue remain open.
The working manuscript contains the reviewed forward study and a prospective
gradient/optimization protocol. Harmonic coupling, shape-gradient verification
and matched optimization timing are not reported as completed experiments.

## Read and build

Open `main.pdf`. From this directory:

```sh
python3 verify.py
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render.py
pdflatex -interaction=nonstopmode -halt-on-error main.tex
bibtex main
pdflatex -interaction=nonstopmode -halt-on-error main.tex
pdflatex -interaction=nonstopmode -halt-on-error main.tex
```

The repository's `papers/build.sh` also builds this version. Figure regeneration
checks input hashes, recomputes mean/ripple statistics, and writes three PDF/PNG
figures and two LaTeX tables. It does not rerun the FEM solver. `verify.py` is
read-only and uses the Python standard library.

## Evidence

`evidence-manifest.json` pins the inputs to GEODE commit
`581ee7ad40c9646f43e0d6df3de7b5a87d06f798`, with per-file SHA-256 hashes.
Copies are in `figures/src/data/` so later solver work cannot change this version's
figures. The original benchmark directories contain physical rerun instructions.
The source reference is ICEMsnapshot commit
`b145aaf4ab1a6424828bc941eaf172f48fa0d458`; the upstream GPL-3.0 license is
included beside the copied data. No upstream MATLAB implementation is copied.

The committed raw records contain 500 solves (240 baseline plus 260
sensitivity), all with tolerance status. The benchmark README reported 520;
this manuscript uses the count verified directly from the nine JSON files.

The mean/ripple headline values and tables derive from raw runs. The empirical
refinement intervals are not proven error bounds. Timings are descriptive
forward costs, not a comparative speedup. The cost table sums the individual
row timings; that differs slightly from the enclosing run timer in the original
README because it excludes between-solve work.

Published citation metadata was checked against Crossref; its response records
are in the thread's `refs/bibliography-metadata.json`. Claim support is grounded
in the existing primary-source literature screen and the cited papers. A formal
citation audit and independent manuscript review remain to be done.

## Next revision

- Incorporate the harmonic-coupling and reference-refinement experiments.
- Verify motor shape gradients, then compare useful design spaces and optimizers.
- Fix hardware, thread limits, timing repetitions, design constraints and target margins.
- Finalize authorship and venue with the operator.
- Run independent Anvil review, citation/numerical audit and rendered-figure review.

Keep this completed version immutable; new results belong in the next version.
