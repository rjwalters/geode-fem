# Verdict — transmon-benchmark.4

**Total: 40 / 44**

**Decision: `advance: false`** — total ≥ 35, but one unresolved critical flag
short-circuits the verdict per the rubric.

## Critical flags

- **`numerical_inconsistency_scale_story` — the PR #557 scale-disclosure
  correction was applied to §8.2 only; six other sites still assert the
  retracted causal story.** §8.2 now states that the earlier 1.16M-DOF
  OOM figure "was a small-box truncation artifact", that given a 128 GB box
  the direct solve "does complete" at 565.5 s / 92.2 GB peak RSS, that it
  "loses to Palace on both axes" (423.12 s / ~33 GB aggregate), and that the
  crossover "is a flop-and-fill deficit, not merely a memory wall." Yet the
  abstract still claims the advantage inverts "at ${\sim}1.16$M DOFs where
  the direct factorization is OOM-killed and Palace completes"; the
  contributions bullet says it "is memory-bound"; Table 3's caption and rows
  present the OOM-killed 63.9 GB cell as the large-scale finding with no
  completion row; the trade-off paragraph says "Palace wins at scale on
  memory, completing where the direct path OOMs"; the Discussion repeats
  "memory-bound"; and Limitations (iv) repeats "OOM-killed." A sophisticated
  reader who reaches §8.2 discovers the abstract, the headline table, and the
  limitations section all carry a causal claim the paper itself has retracted
  — precisely the "number/claim in the text disagrees with the corresponding
  figure or table" flag class, compounded by the paper's own stated rule that
  "Numbers in text must match the tables exactly." The six sites and the fix
  are enumerated in comments.md (blocker entry).

No other flag class fires: the reviewer independently compiled the current
source to a clean fixpoint (24 pp, zero undefined citations/references — no
build flag, though the shipped `main.pdf` is stale, see findings.md); all 50
cite keys resolve with complete entries; the deterministic numeric detector
and manual arithmetic spot-checks pass; no close prior work for v4's claims
is ignored (litsearch-substrate-backed).

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 6 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 3 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **40** |

Full justifications with verbatim quoted evidence in `scoring.md`.

## Venue overlay (advisory)

Advisory venue overlay scored **9/10** against `anvil-pub-arxiv-v1`
(citation completeness 3/3, reproducibility 2/3 — repo URL/DOI still
TODO(operator), clarity of contribution 2/2, scope/category 2/2 for
physics.comp-ph); no venue critical flags. See `_review.venue.json`. Advisory
only — does not affect the convergence gate.

## Top 3 revision priorities (advance: false)

1. **Clear the flag: propagate the #557 correction to all six stale sites**
   (abstract, contributions bullet, Table 3 caption/rows, trade-off
   paragraph, Discussion, Limitations (iv)) so the whole paper tells the
   corrected flop-and-fill story — and do it as part of, not before, the v5
   reframe so the new abstract never re-imports the memory-wall claim.
   Site-by-site fix list in comments.md (blocker entry).
2. **Execute the 2026-07-16 BRIEF reframe** (differentiable transmon design,
   LOM branch) against the spine at `docs/research/transmon-paper-reframe.md`
   — the twelve-item delta list in comments.md §A: new title/abstract/intro
   lead, the PROVEN 2×2 adjoint matrix + capacitance→E_C chain + diffopt
   centerpiece + real-device honest negative (with committed artifact paths),
   cross-validation demoted to credential, matrix-free scale story retired to
   roadmap, LOM-now/eigenmode-roadmap scope discipline.
3. **Re-run `pub-litsearch` for the differentiable-design axis before
   drafting v5 related work** — the BRIEF's lead identifiers are partially
   misaligned (verified read-only against the arXiv API: 2408.12704 is not
   QDesignOptimizer; 2312.13483 is SQuADDS, already cited; 2407.10273 is not
   FDTDX); leads and caveats in comments.md §B. Then compress toward the
   8–12-page target (dim 9) while re-rendering `main.pdf` (stale artifact,
   findings.md).
