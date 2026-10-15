# v14 author-side checks, 2026-10-14

These are author-side checks of transcription, figures and packaging, not an
independent review, audit or numeric critic, and no score is claimed. No
solver was run for v14 and no `cargo` was run. One stdlib Python analysis was
added and run: `benchmarks/transmon_eigen/palace_crack/palace_crack_edges.py`
on the committed fixture mesh (metal group 5 only), output committed as `fixture_crack_edges.toml`.

Environment: Python 3.14, NumPy 2.4.1, Matplotlib 3.10.8; Poppler; TeX Live / pdfTeX.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.14`: PASS,
  **18 pages** (v13: 17). `main.log`: 0 overfull boxes, no undefined
  reference or citation. `main.blg`: 0 warnings. 37 bibliography entries.
  The abstract is 199 words.
- `python3.14 verify_evidence.py`: PASS (crack counts exact against Palace's log: 1,007 edges and vertices, 15,098 boundary elements; a 1,009 edit is killed), with the v14 guards listed in
  `changelog.md`. Mutations killed: "code verification" reinserted in §4, an
  unqualified "same discrete problem" sentence in §1, and the Palace-only
  Newton reset to 3 in Table 3.
- `python3.14 verify_figures.py`: PASS. Figures 3 and 5 were regenerated
  byte-identically. Figures 1, 2 and 4 are byte-identical to v9 (Figure 1) and
  v13 (Figures 2 and 4).
- STEP 0 (certificate L¹ terms): a code reading of `electrostatic_dual.rs`
  shows a rigorous upper bound via |S|^½‖·‖_{L²} with exact quadrature of the
  squared polygonal integrands. See `changelog.md`.
- Citations: Koch et al. 2007 and Minev et al. 2021 were resolved via Crossref
  (title, venue, volume, article number, year, authors) on 2026-10-14.
- v1–v13 and every critic sibling are unchanged (`git diff --stat main --
  papers/transmon-benchmark/` touches only `transmon-benchmark.14/`).
