# v7 preparation checks — 2026-10-10

These are author-side checks, not an independent review or paper audit.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.7`: PASS.
  Initial clean build converged after four pdflatex passes; final build after
  path-wrapping changes converged after three. BibTeX succeeds. The final PDF
  is 26 pages. No undefined references/citations or `??` in extracted text.
- Final `main.log`: one inherited 1.13197 pt overfull hbox in the contribution
  list, below the paper render gate's 5 pt threshold. The long-path overflow
  introduced by the expanded artifact list was fixed with `xurl`.
- `python3 verify_evidence.py` (Python 3.12): PASS. CPU and AMS table values,
  mode/repeat counts, harmonic arithmetic, local port-subspace numbers,
  unchanged bibliography/citation set, operator-marker preservation.
- CPU figure regenerated from the committed m6i TOML using its new source
  script (Python 3.12, matplotlib). All three geode routes and both Palace
  field-output settings are present; error bars are observed min/max.
- Rendered pages 15–17 inspected: pending section, CPU table/chart and AMS
  table are legible and within page margins. The different panel x-axis
  ranges are explicitly labeled. This was targeted inspection, not a full
  re-audit of all inherited pages.
- Standalone package check: copied only `main.tex`, `main.bbl`, `refs.bib`,
  `anvil-paper.cls` and `figures/` into an empty temporary directory, removed
  `TEXINPUTS`, and ran plain pdflatex without BibTeX. PASS after three passes;
  no undefined citations/references or `??`. This checks the committed BBL
  and vendored class without using another paper's directory.
- Historical v6 tracked files checked byte-for-byte against branch-base
  HEAD: unchanged. No old critic directories edited.
- Anvil evidence-drift baseline recorded using the installed helper.
- Anvil pending-marker check: **expected exit 1**, one active source
  `issue-1034`; generated `.7.pending/_review.json` records it. The marker
  remains visible in the PDF and prevents READY/AUDITED terminal status.

Remaining work: the #1034 experiment; independent v7 paper review/audit;
the existing operator submission inputs; the previous audit's off-disk
citation-support limitations; and final length/venue decisions. No Rust
solver tests or new benchmarks were run: this change consumes committed
results and changes paper artifacts only.
