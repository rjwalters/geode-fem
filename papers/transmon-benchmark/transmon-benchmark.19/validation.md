# v19 author-side checks, 2026-10-22

Author-side checks of transcription, figures and packaging, not an independent
review; no score is claimed. No solver and no `cargo` was run. Inputs: `main` at
`56224d6b`. Python 3.14, run as `python3.14 -I`.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.19`: PASS, **20
  pages** (v18: 19), references from page 19; 0 overfull boxes; no undefined
  references; no "??". Abstract 200 words.
- `verify_evidence.py`: PASS (with `check_v19()`; v18 `main.tex` pinned by
  SHA-256 beside v10–v17). Mutations of `main.tex`, each killed: "on uniform
  meshes" dropped from the abstract's rate clause; "order-1" reintroduced in the
  definition of $p$; derivative credit ("with geode-fem input") added to the
  conclusion; the affiliation marker changed; "at order 2" added to the family-b
  paragraph; a second contribution added to C.D.
- `verify_figures.py`: PASS; all five figures redrawn and regenerated
  byte-identically; `check_v19_coordinates()` reads Figure 1's strip and box
  from `fd_initial.toml`, Figure 2's pair extrapolations against the committed
  estimates, and Figure 4(b)'s sequences from the bracket artifacts; no
  figure contains the word "order".
- Every page rendered at 110 dpi and inspected (Figures 1–5, Table 1, the
  author block, Section 8).
- v1–v18 unchanged.
