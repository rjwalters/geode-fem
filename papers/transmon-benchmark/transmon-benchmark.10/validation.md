# v10 author-side checks — 2026-10-13 (tracker #1063)

These are author-side checks of transcription, figure generation and
packaging. They are not an independent review, audit or numeric critic, and no
score is claimed. No geode or Palace solve and no benchmark generator was run,
and no cloud resource was used: v10 consumes artifacts already committed on
`main` (`264b99b8`).

Environment: Python 3.14 with NumPy 2.4.1 and Matplotlib 3.10.8; Poppler
(`pdftotext`, `pdfinfo`, `pdftoppm`); TeX Live / pdfTeX.

- **Started from v9**, restructured as listed in `changelog.md`. Every number is
  read from a committed artifact; derived values are listed in `provenance.md`
  and recomputed by the verifier.
- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.10`: PASS,
  three pdflatex passes, **23 pages** (v9: 43). `main.log` has no undefined
  reference or citation and no overfull box; the extracted text has no `??`.
- `python3.14 verify_evidence.py`: PASS. Pins Table 1 and the fixture
  refinement shifts; the cross-check (published pairs, reproduction, the
  estimate and band recomputed from the ladder cells, Table 2 row by row, the
  capped band, same-mesh agreement, deficits, observed orders); the static
  route (Table 3, both certified brackets re-derived with their rounding, the
  certified gap from the unrounded bounds, the overstatement floored, the cap
  recomputed from the junction constants, the 0.22 % offset); the
  re-optimization (Table 4 rows with err recomputed and the x₁, x₂ variables,
  the criterion step and run counts, the gradient check, Table 5 rows, the
  converged design, the slope from `opt_log.jsonl`, the c range and its
  percentages, the shared-limit order and value solved for, the ladder hours,
  the PRIMA end-point err); Appendix A (scalar and tensor brackets, Palace
  agreement recomputed from `palace_validation.json`, no anchor-chain value
  in the body); Appendix B (CPU table, AMS ratios, GPU cell ordering, the
  1.16M run log); the citation set equals `refs.bib`, litsearch entries are
  unchanged on merge, `webb2000design` is not cited; the operator items
  (title and author block identical to v6 and v9; the six inherited
  `TODO(operator)` lines verbatim; exactly one new `TODO(operator)` line, pinned;
  both validation-claim sentences verbatim and present in v9; the
  acknowledgments identical to v9; v9's `main.tex` SHA-256 pinned); voice and
  audience rules (exactly three em-dashes outside comments, all inherited; no
  virtue labels, revision narrative or issue numbers in the body; abstract
  ≤ 250 words; the headline and the not-a-bound labels pinned).
- `python3.14 verify_figures.py`: PASS. Input SHA-256s of seven committed
  artifacts; all four scripts run and every PDF regenerates byte-for-byte;
  Figure 1 and its three source files are byte-identical to v9's; widths
  ≤ 6.3 in; figure text against the artifacts (Figure 2's four estimates with
  their percentages and the not-a-bound label, Figure 3's two brackets and the
  2.5–3.0 % gap with no design target, Figure 4's labels); inclusion order and
  caption wording.
- **Mutation check:** 45 single edits, all caught. 36 to `main.tex` in a
  temporary copy of the version directory (Table 2 estimate and band, the
  tensor bracket, the certified gap, the overstatement, the cap, the run
  count, +19 %, the c range, two not-a-bound labels, a Table 4 err, the
  shared-limit order, the ladder hours, the title, an inherited
  `TODO(operator)`, the new `TODO(operator)` removed, the validation clause, the
  acknowledgments, an inserted "honest", an inserted revision-history sentence,
  an inserted issue number, an inserted em-dash, an inserted "uncontested", a
  citation of `webb2000design`, the Table 1 qubit Δ, a refinement shift, the
  static midpoint, the overstatement in fF, a design tensor bracket, the FD
  error rounded down, a CPU cell, a misread-target value in the body, the
  "no design meaning" sentence, "not a verified hit" inverted, a figure
  caption's wording and the figure order); 4 to figures (a label edited and
  regenerated, a script edited without regenerating, stale PDF bytes, a
  provenance hash); 4 to committed files restored afterwards (the converged
  estimate, the tensor bracket, a Newton err, v9's `main.tex`). The harness is
  scratch and not committed.
- **Visual inspection** of every page of the built PDF at 80 dpi, with
  Figure 1 (p. 5), Figure 2 (p. 8), Figure 3 (p. 11) and Figure 4 (p. 14) also
  checked as standalone renders while drawing. `figure_style.save()` rejected
  six drafts of Figures 3 and 4 (an off-canvas axis label, a legend over tick
  labels, three annotation overlaps, overlapping inset ticks caught by eye).
- **Anvil pre-flights** (from the main checkout's `.anvil`):
  `pending_marker --write-review` exit 0, no marker,
  `.10.pending/_review.json` written by the tool, same shape as v9's;
  `audience_check` 0 hits; `numeric_consistency` 0 findings.
- **Immutability:** `git status` touches only `transmon-benchmark.10/`,
  `transmon-benchmark.10.pending/`, `BRIEF.md` and `papers/README.md`.

Not checked: no generator was re-run; the certified lower bounds, the ladder
cells, the coupling and loop-inductance extrapolations and the README prose
values (δ noise, second δ estimate) are transcribed, not recomputed; the
sensitivity-layer FD errors (Table 6) are carried from v9; no Anvil critic was
run.
