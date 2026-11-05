# v23 author-side checks, 2026-10-23

Author-side checks of transcription and packaging, not an independent review;
no score is claimed. No solver, no `cargo` and no new computation was run.
Inputs: `main` at `c1b17a23`. Python 3.14, run as `python3.14 -I`.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.23`: PASS, **21
  pages** (v22: 21); 0 overfull boxes; no undefined references; `main.bbl`
  byte-identical to v22's.
- `verify_evidence.py`: PASS (with `check_v23()`; v22 `main.tex` pinned by
  SHA-256 beside v10–v21; the multiset of numerals in the body, 1,005 tokens,
  equals v22's; title, author block, date-and-DOI block, citation set and
  `refs.bib` are v22's; section, subsection and moved-block order; one copy of
  each applied wording; the Computed tier; the #1231 nits). Mutation check: one
  digit changed in Section 1 ($5.591 \to 5.519$) fails the numeral pin; reverted.
- `verify_figures.py`: PASS; the figures are v20's bytes (v21, v22 and v23
  changed none).
- Abstract: 199 words (cap 200).
- No `[PENDING` marker.
- Pages 2, 10 and 19 rendered and inspected: the solver paragraph before the
  result paragraph and the tier paragraph after it; Figure 3 at the top of page
  10 between the family-b and adaptive paragraphs; Conclusion, Reproducibility,
  Acknowledgments and the contributions section (11) in that order.
- v1–v22 unchanged.
