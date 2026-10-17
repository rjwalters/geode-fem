# Line-level comments — transmon-benchmark.6

Keyed to `main.tex` sections (single-file document; no `\input`/`\include`
children). Grouped by severity. v6 is a sanctioned surgical revision of v5;
comments on untouched material are carried forward only where still live.

## blocker

None.

## major

None. (The v5 major — the `eriksson2025automated` misquotation, §1 — is
resolved: the span is unquoted to an accurate paraphrase, "iterates
time-consuming electromagnetic simulations of HFSS-class solvers"
(L127–128), and no remaining ``...'' span in the body asserts cited-source
words: L160 "geode-fem" is the solver's name, L756 "FD rel-err" is a
table-column name, L1069 "peak" and L1112 "never completes" are scare
quotes on the paper's own words.)

## minor

- **§Reproducibility & artifacts / front matter + §8.3 + acknowledgments —
  `TODO(operator)` markers** (L70, L74, L387, L1209, L1401–1402): all
  remain hard arXiv submission blockers (title wording, affiliations,
  cubecl f64 tracking-issue URL, repository URL/DOI, acknowledgment +
  whiteroom decision). Operator-owned by thread convention; unchanged from
  v5 and correctly carried verbatim per the changelog.
- **§14 Conclusion — recap density** ("Around the capability we kept the
  honest ledger" — L1383): the conclusion still re-walks the abstract and
  contribution list nearly item for item, and the paper is 25 compiled
  single-column pages against the BRIEF's ~15-single-column-equivalent
  target. The changelog's declination for the final surgical pass is
  reasonable; the compression levers (conclusion recap to 3–4 sentences,
  Discussion credential-paragraph trim, §6.2 arc, two-column venue layout)
  remain documented for any post-thread polish.
- **Citations unverifiable on-disk**: 49 of 57 entries have no source PDFs
  under `refs/`; the v5 audit verified the 7 newly-merged keys plus the
  flagged QDO abstract live, but the remainder should be author-verified
  before submission. Carried from the v5 audit note; no action available
  to the reviser.

## nit

- **Figure filename/number skew** (`fig4-cpu-wallclock.pdf` renders as
  Figure 6; `fig6-participation.pdf` as Figure 4): cosmetic source-map
  skew only, no reader-visible defect; the changelog's declination
  (hardcoded output filenames in the figure scripts; re-render risk on the
  last allowed iteration) is sound. Rename at a convenient post-thread
  pass.
- **Fig. 6 inset bar labels** round to integers vs the caption's 28.7
  core-s — consistent with the stated displayed-value convention; carried
  as declined (figure re-render out of scope for the surgical pass).
- Procedural: render-gate (step 4b) ran fail-open — no
  `transmon-benchmark.6.audit/compile-log.txt` exists yet (review precedes
  audit for this iteration); the reviewer substituted a fresh scratch-copy
  compile to fixpoint (4 passes, clean; one 1.13 pt overfull hbox; shipped
  PDF current). `_gate.json` is therefore not emitted; the audit's own
  compile loop will produce the canonical gate artifacts.
