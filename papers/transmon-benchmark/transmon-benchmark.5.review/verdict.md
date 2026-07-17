# Verdict — transmon-benchmark.5

**Total: 41 / 44**

**Decision: `advance: true`** — total ≥ 35 and no unresolved critical flag.

## Critical flags

None.

**Re-evaluation of the v4 flag `numerical_inconsistency_scale_story`
(required before the threshold check applies): RESOLVED.** The reviewer
re-verified all six flagged sites in the v5 body against the corrected
record (completes at 565.5 s / 92.2 GB on a 128 GB box per the committed
`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`, but loses
to Palace at 423.12 s / ~33 GB aggregate on both axes; flop-and-fill
crossover below 1M DOFs):

1. **Abstract** (L97–99) — no memory-wall/OOM claim; now "no corner where
   GEODE-FEM beats Palace at scale, and we report it honestly."
2. **Contributions bullet** (L215–222) — "completes given adequate memory
   ($565.5$\,s, $92.2$\,GB peak) but Palace is faster and far leaner
   ($423.12$\,s, ${\sim}33$\,GB aggregate); a flop-and-fill crossover below
   1M DOFs, not merely a memory wall."
3. **Table 3** (L1037–1056) — now carries BOTH large-scale geode rows
   ("killed at ceiling / 63.9 GB (truncated)" AND the 128 GB-box completion
   "565.5 / 92.2 GB") with a dagger note marking the box-protocol departure
   and identifying the 63.9 GB figure as "a truncation of the true
   footprint, not a measurement of it"; the caption states the
   completes-but-loses finding and cites the committed log.
4. **Trade-off paragraph** (L1093–1096) — stated symmetrically: "Palace
   wins at scale on both wall clock and memory."
5. **Discussion** — no residual memory-bound claim; the scale statement
   appears only in corrected form (§11.1, Threats to validity L1309–1312,
   Limitations).
6. **Limitations (iv)** (L1323–1327) — "completes given memory but loses to
   the distributed iterative reference on both wall clock and memory — a
   flop-and-fill crossover below 1M DOFs, per the committed 128 GB-box
   measurement."

Every remaining occurrence of "OOM"/"63.9 GB"/"memory wall" in the body is
inside the explicit retraction framing ("was a small-box truncation
artifact and is retracted in favor of this measurement", L1089–1091) or
the header provenance comment. The deterministic numeric detector (573
numbers, 0 findings) and manual artifact cross-checks (findings.md) pass.
The corrected record is consistent everywhere it appears.

No other flag class fires: the reviewer compiled the source to a clean
fixpoint (25 pp; zero undefined citations/references; the shipped
`main.pdf` matches the fresh compile — the v4 staleness defect is fixed);
all 57 cite keys resolve with complete entries and zero unused; every
spot-checked number in the new §7–§10 content traces exactly to its
committed artifact; the litsearch claim-precision cautions are applied
correctly (SQcircuit line credited with solving lumped-level eigenpair
gradients; QDO cited once for both roles; xue2023jax kept at CPC 2023);
and the LOM-now/eigenmode-roadmap boundary is honored with no overclaim.

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
| 8 | Citation hygiene | 5 | 4 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **41** |

Full justifications with verbatim quoted evidence in `scoring.md`.

## Venue overlay (advisory)

Advisory venue overlay scored **9/10** against `anvil-pub-arxiv-v1`
(citation completeness 3/3 — the differentiable-design axis is now
litsearch-substrate-covered; reproducibility 2/3 — repo URL/DOI still
TODO(operator); clarity of contribution 2/2; scope/category 2/2 for
physics.comp-ph, with the quant-ph cross-list strengthened by the
reframe); no venue critical flags. See `_review.venue.json`. Advisory
only — does not affect the convergence gate.

## Top revision priorities (advisory — paper advances to pub-audit)

Not required for advancement, but the highest-leverage items for the
audit/polish pass (details in comments.md):

1. **Fix the inexact quotation of `eriksson2025automated` in §1** — the
   quoted span "time-consuming electromagnetic simulations" drops the word
   "iterative" from the source abstract ("time-consuming iterative
   electromagnetic simulations"); quote verbatim or unquote to paraphrase.
   The QDO authors are in this paper's outreach audience.
2. **Resolve the five TODO(operator) markers** — all are hard arXiv
   submission blockers (title wording, affiliations, cubecl f64 tracking
   URL, repo URL/DOI, acknowledgment/whiteroom decision).
3. **Length** — 25 pp single-column vs the BRIEF's ~15 single-column
   target; the venue two-column layout plus the conclusion-recap and
   Discussion-restatement trims in comments.md are the remaining levers.
