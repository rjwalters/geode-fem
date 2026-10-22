# v21 author-side checks, 2026-10-22

Author-side checks of transcription and packaging, not an independent review;
no score is claimed. No solver, no `cargo` and no new computation was run.
Inputs: `main` at `9c4db241`. Python 3.14, run as `python3.14 -I`.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.21`: PASS, **21
  pages** (v20: 21); 0 overfull boxes; no undefined references; `main.bbl`
  byte-identical to v20's.
- `verify_evidence.py`: PASS (with `check_v21()`; v20 `main.tex` pinned by
  SHA-256 beside v10–v19; v21 equal to v20 apart from the header comment and
  the author and date blocks and the Reproducibility footnote, which now
  gives the DOI in place of the deposit placeholder and its TODO comment).
- `verify_figures.py`: PASS; the figures are v20's bytes.
- `pending_marker` (Anvil): pass, zero markers.
- Page 1 rendered and inspected: each author with their own affiliation, and
  the DOI line under the date.
- v1–v20 unchanged.
