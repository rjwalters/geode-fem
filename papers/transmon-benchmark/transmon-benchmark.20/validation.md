# v20 author-side checks, 2026-10-22

Author-side checks of transcription, figures and packaging, not an independent
review; no score is claimed. No solver, no `cargo` and no new computation was
run. Inputs: `main` at `e74285bf`. Python 3.14, run as `python3.14 -I`.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.20`: PASS, **21
  pages** (v19: 20), references from page 20; 0 overfull boxes; no undefined
  references; no "??". Abstract 199 words.
- `verify_evidence.py`: PASS (with `check_v20()`; v19 `main.tex` pinned by
  SHA-256 beside v10–v18). Mutations of `main.tex`, each killed:
  - "qubit frequencies" reverted to "qubits" in the abstract;
  - "reaches qubits" restored in Section 1;
  - "which sets no single-precision option" dropped;
  - the `[PENDING coauthor-affiliation]` marker restored;
  - the qubit-frequency definition removed from Section 1's first paragraph;
  - the Section 1 pointer to the Discussion removed.
- `verify_figures.py`: PASS. Figures 1, 2 and 5 were regenerated
  byte-identically and differ from v19; Figures 3 and 4 and their scripts are
  v19's bytes. Mutations, each killed by `check_v20_colours()` on its own, not
  only by the byte check:
  - the "Junction" label set back to black;
  - the junction sheet in (b) set back to orange;
  - the stretched-strip label set back to blue;
  - Figure 2's two-marker key removed;
  - Figure 5's extrapolated shape-derivative steps set back to green;
  - the mapped band set back to green;
  - the $L_J$ coil in Figure 1(d) set back to blue.
- Every edited page rendered and inspected: page 1 (author block, abstract,
  Section 1), Figures 1, 2 and 5, page 17 (the new paragraph) and page 19.
- v1–v19 unchanged.
