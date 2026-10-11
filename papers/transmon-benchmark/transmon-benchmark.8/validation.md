# v8 author-side validation — 2026-10-11 UTC

These checks cover transcription, figure generation, and document packaging.
They are not an independent scientific review or audit. No solver benchmarks
or Rust tests were run; this change only consumes recorded numerical evidence.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.8`: PASS.
  Final PDF: 33 pages. No overfull boxes, undefined citations/references, or
  LaTeX errors in the final log.
- `python3.14 .../verify_evidence.py`: PASS. Inherited CPU/AMS tables, mode and
  repeat counts, harmonic arithmetic, qubit/Palace corrections, unknown counts,
  coarse-mesh caveats, previous multi-parameter results, pending markers,
  unchanged bibliography/citation set, and operator-marker preservation.
- `python3.14 .../verify_figures.py`: PASS. All pinned input SHA-256 hashes;
  three-step trace and validation consistency; finer-mesh capacitance and
  gradient changes below the recorded 1% working tolerance; actual polygon
  dimensions; precise qubit residual; manuscript transcriptions; nonempty PDF
  text; and byte-identical regeneration of all seven figure PDFs.
- Source snapshot check: the three copied experimental JSON files match
  `1b445770abd268e44c542e9363047583c8b6e9b5` byte-for-byte. The derived island
  surface snapshot is explicitly labeled as a local export with its own hash.
- Visual inspection: all seven rendered figure pages (7, 10, 11, 14, 20, 23,
  28) inspected at 1600-pixel page height, plus standalone figure previews.
  Labels/legends/captions are readable, annotations stay within the plots,
  and axes and mode identities match the stated comparisons. This is a
  figure-focused pass, not a fresh audit of every inherited page.
- Standalone package: copy only `main.tex`, `main.bbl`, `refs.bib`,
  `anvil-paper.cls`, and `figures/` to an empty temporary directory; unset
  `TEXINPUTS`; run plain pdflatex three times without BibTeX: PASS. No undefined
  references, overfull boxes, LaTeX errors, or `??` in extracted text.
- Anvil evidence-drift baseline recorded using the installed helper.
- Anvil pending-marker check: expected exit 1, with `issue-1051` and
  `issue-1037` still active. The generated `.8.pending/_review.json` records
  them. They prevent terminal READY/AUDITED status.
- Historical versions and critic siblings are unchanged; the diff is confined
  to the new v8 directory and its generated pending-marker sidecar.

Environment: Python 3.14, NumPy, Matplotlib 3.10.8, Poppler (`pdftotext` and
`pdftoppm`), TeX Live/pdfTeX. Deterministic PDF checks apply to this installed
rendering environment; different library or font versions can change bytes.

Remaining: independent review of PR #1058 and this paper revision, continuum
convergence (#1051), two-target optimization (#1037), existing operator inputs,
and inherited citation-support limitations. Local P2 checks do not certify a
continuum optimum, tensor shape optimization, or full-wave inverse design.
