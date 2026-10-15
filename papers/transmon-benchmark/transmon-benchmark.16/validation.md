# v16 author-side checks, 2026-10-15

These are author-side checks of transcription, figures and packaging, not an
independent review, audit or numeric critic, and no score is claimed. No
solver and no `cargo` was run for v16, and no committed artifact changed.

Environment: Python 3.14, NumPy 2.4.1, Matplotlib 3.10.8; Poppler; TeX Live / pdfTeX.

- Step 0 (Fable M1), read before drafting: `electrostatic_dual.rs` module docs
  and `certify_impl`, and `examples/transmon_c_sigma_bracket.rs`. The static
  model's box is natural (zero normal flux), boundary faces are penalized
  one-sided (`boundary_flux_l1`), and the ground set is the metal sheets only.
  The lower bound transfers to the grounded (PEC) box by monotonicity of the
  minimum energy in the admissible set. See changelog.md, Step 0.
- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.16`: PASS,
  **19 pages** (v15: 19), body to page 18, references from page 18.
  `main.log`: 0 overfull boxes, no undefined reference or citation.
  `main.blg`: 0 warnings. 42 bibliography entries (v15: 39). The abstract is
  200 words. No page is under-filled; the largest internal gap on any page is
  about three lines (the queued half-blank-page item is resolved).
- `python3.14 verify_evidence.py`: PASS, with the v16 guards listed in its
  docstring and `check_v16()`. Mutations, each killed: "4.2 %" reverted to
  4.1 %; the box term dropped from Eq. (3); the natural-box clause removed;
  "fused primitives" restored; the E_C upper end changed to 0.2277; Table 2's
  family-b final qubit changed to 3.293; the abstract's "(17–39 % within the
  band)" removed; "0.61" claimed for both family-a qubits; the monotonicity
  sentence weakened to "may raise"; the GCI sentence de-attributed; the 16.6 %
  changed to 14 %; the full-model 15.6–16.1 % restated as "at least 16.6 %";
  the full-model $C_\Sigma$ band and the static-route 85.8–86.1 fF altered.
- `python3.14 verify_figures.py`: PASS. All five scripts regenerate
  byte-identical PDFs. Figures 3 and 5 are v15's (= v14's) bytes, Figure 4 is
  v15's; Figures 1 and 2 changed (Figure 2: label placement and 0.3 in less
  height; Figure 1: 0.45 in less height, panel (c) shorter; no font changes).
  The new Figure 2 guard tests every line with its own transform (a first
  version skipped every data series; corrected), and fails on v15's label
  placement, through the overlap with "Palace, order 2" even with the
  slot-centre test disabled, and on a label centred on the estimate. Figure 2 checked visually at 130 and 300 dpi, Figure 1 at
  110 dpi.
- Anvil pre-flights on the v16 dir: `audience_check` 0 hits in all classes,
  pass; `numeric_consistency` 811 numbers, 0 findings, pass.
- Citations: Richardson 1911 (10.1098/rsta.1911.0009) and Roache 1994
  (10.1115/1.2910291) resolved on Crossref 2026-10-15 (title, venue, volume,
  pages, year, author). Minev et al. 2021, arXiv:2103.10344, read on the arXiv
  API and DataCite (title, authors, year, abstract); no journal version on
  Crossref. Ye et al. (arXiv:2511.09041) and Minev et al. 2021
  (arXiv:2010.00620) full texts read for the convergence sentences. The
  whiteroom layer definitions read from `crutcher/palace_whiteroom` `CLAUDE.md`
  through the GitHub API.
- v1–v15 and every critic sibling are unchanged (`git diff --stat main --
  papers/transmon-benchmark/` touches only `transmon-benchmark.16/`; the
  verifier also pins v10–v15 `main.tex` by SHA-256).
