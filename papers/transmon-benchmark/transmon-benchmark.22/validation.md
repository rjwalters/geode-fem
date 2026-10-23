# v22 author-side checks, 2026-10-22

Author-side checks of transcription and packaging, not an independent review;
no score is claimed. No solver, no `cargo` and no new computation was run.
Inputs: `main` at `52da100a`. Python 3.14, run as `python3.14 -I`.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.22`: PASS, **21
  pages** (v21: 21); 0 overfull boxes; no undefined references; `main.bbl`
  byte-identical to v21's.
- `verify_evidence.py`: PASS (with `check_v22()`; v21 `main.tex` pinned by
  SHA-256 beside v10–v20; v22 equal to v21 apart from the header comment and
  the title, which is now "An Independent Finite-Element Cross-Check of the
  DeviceLayout.jl Transmon").
- `verify_figures.py`: PASS; the figures are v21's bytes.
- No `[PENDING` marker.
- Page 1 rendered and inspected: the new title, with the author and DOI blocks
  unchanged.
- v1–v21 unchanged.
