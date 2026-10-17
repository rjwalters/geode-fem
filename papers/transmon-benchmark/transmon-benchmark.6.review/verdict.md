# Verdict — transmon-benchmark.6

**Total: 42 / 44**

**Decision: `advance: true`** — total ≥ 35 and no unresolved critical flag.

## Critical flags

None.

**Re-evaluation of the v5 audit flag (Misquotation of a cited source,
`eriksson2025automated`) — required before the threshold check applies:
RESOLVED.** The v5 body attributed the quotation "time-consuming
electromagnetic simulations" to arXiv:2508.18027, whose abstract reads
"time-consuming *iterative* electromagnetic simulations" — quotation marks
asserting words the source does not contain. v6 takes the second of the two
fixes the audit itself prescribed ("remove the quotation marks and
paraphrase"): §1 now reads, unquoted, "iterates time-consuming
electromagnetic simulations of HFSS-class solvers" (L127–128). The
paraphrase no longer asserts verbatim source words, "iterates" preserves the
source's "iterative", and the surrounding substance (HFSS-class solver in
the loop; separate, user-defined analytic physics models guiding parameter
updates) matches the source abstract as recorded in the v5 audit's
first-hand arXiv verification. The reviewer additionally verified the
changelog's claim that no other quotation in the paper asserts cited-source
words: the four remaining ``...'' spans (L160, L756, L1069, L1112) are the
solver's own name, a table-column name, and two scare quotes on the paper's
own words.

**Surgical-diff integrity check (iteration 6 of 6): CLEAN.** The full diff
of `transmon-benchmark.6/main.tex` against `transmon-benchmark.5/main.tex`
contains exactly the four changelog-declared changes plus the updated header
provenance comment (with the v5 header retained under a "historical" marker)
— no stray tokens, no unexplained drift; the reviser's self-reported
"placeholder-sentinel" insertion is confirmed reverted (absent from the
body; the only "placeholder" strings are the pre-existing figure-placeholder
macro, unchanged from v5). `refs.bib`, `anvil-paper.cls`, `main.bbl`, and
all `figures/` are byte-identical to v5. All four changes verified against
their committed artifacts:

1. **eriksson2025automated unquote → paraphrase** (§1 L127–128) — resolved,
   see the flag re-evaluation above.
2. **Table 3 off-target Peak RSS provenance footnote** (L1049–1052,
   L1073–1078) — honest against the artifact:
   `benchmarks/transmon_bench_cpu/results.toml` `[matched.off_target.*]`
   (L100–126) records `wall_s` only (36.8/26.6/248.0/64.7, matching the
   table), while `[matched.physical_target.*]` records `peak_rss_gb = 3.1`
   (geode, both thread counts) and `peak_rss_gb_per_rank = 0.5` (Palace,
   both rank counts) at `n_interior_dofs = 133108` — exactly the footnote's
   "repeat the physical-target measurements of the same session and the
   same 133,108-DOF pencil, and are not independently recorded in the
   artifact."
3. **137× → 136×** (§8.3 L1145–1150) — matches the artifact's own HONEST
   READ comment in `benchmarks/gpu_driven_scaling/results.toml` ("136x
   faster at n=6 (0.032 s vs 4.39 s)"; unrounded medians 4.388126/0.032302
   = 135.86). The companion 44× and 13.5× (1.86 vs 81.8 s; 6.04 vs 81.8 s)
   hold under either basis.
4. **0.2155/0.2156 anchor clarifier** (§9.2 L916–919) — matches both TOMLs:
   `pad_results.toml` `[anchor_attempt]` targets `c_sigma_target_ff = 89.9`
   with `e_c_target_ghz = 0.215464` (→ 0.2155); `results.toml` targets
   `c_target_ff = 89.843364` with `e_c_target_ghz = 0.215600` (→ 0.2156).
   "both values are artifact-faithful" is correct.

Build: the reviewer compiled the v6 source to a clean fixpoint in a scratch
copy (4 pdflatex passes + bibtex; pass 3 still emitted the rerun warning):
zero undefined citations/references, one 1.13 pt overfull hbox (below the
5 pt gate), 25 pages; the shipped `main.pdf` is current (pdftotext stream
identical to the fresh compile). The deterministic numeric detector passes
(578 numbers extracted, 0 findings; sidecar at `transmon-benchmark.6.numeric/`).
All 57 cite keys resolve with complete entries and zero unused.

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 6 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **42** |

Score delta vs v5: 41 → 42 (D8 4→5; the misquote deduction is repaired and
the anchor-proximity minor is resolved). D2 and D9 deductions carry
unchanged — neither was in scope for the surgical pass.

Full justifications with verbatim quoted evidence in `scoring.md`.

## Venue overlay (advisory)

Advisory venue overlay scored **9/10** against `anvil-pub-arxiv-v1`
(citation completeness 3/3; reproducibility 2/3 — repo URL/DOI still
TODO(operator); clarity of contribution 2/2; scope/category 2/2 for
physics.comp-ph with the quant-ph cross-list); no venue critical flags.
See `_review.venue.json`. Advisory only — does not affect the convergence
gate.

## Remaining pre-submission items (advisory — paper advances to pub-audit)

Not required for advancement; the residual items are unchanged from v5 and
operator-owned or explicitly declined for the surgical pass (details in
comments.md):

1. **Five/six `TODO(operator)` markers** (title wording, affiliations,
   cubecl f64 tracking URL, repo URL/DOI, acknowledgment + whiteroom
   decision) — hard arXiv submission blockers, operator inputs.
2. **Length** — 25 pp single-column vs the BRIEF's ~15-single-column
   target; the two-column venue layout and the conclusion/Discussion recap
   trims remain the levers.
3. **49 citations remain unverifiable on-disk** (no PDFs in `refs/`) — the
   v5 audit's note stands; author verification off-disk recommended before
   submission.
