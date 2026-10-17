# Audit flags for transmon-benchmark.5

## Critical flags (block advancement to AUDITED)

- **Misquotation of a cited source** (`\citep{eriksson2025automated}`, §1
  main.tex L113–114): the paper attributes the verbatim quotation
  ``time-consuming electromagnetic simulations'' to QDesignOptimizer's paper,
  but the source abstract reads "time-consuming **iterative** electromagnetic
  simulations requiring manual intervention" — the quoted span drops the word
  "iterative" and is therefore not verbatim. Verified first-hand this audit
  against the live arXiv record (export.arxiv.org, arXiv:2508.18027), not
  merely the litsearch note. This is a claim-support failure for the
  quotation claim itself: quotation marks assert exact words the source does
  not contain, in the paper's opening argument, about a paper whose authors
  are in the outreach audience. The surrounding *substance* is fully
  supported (see citation-audit.md), so the fix is one word: restore
  "iterative" inside the quotation (checking the sentence still reads
  correctly), or remove the quotation marks and paraphrase. The v5 reviewer
  independently identified the same defect (verdict.md, advisory priority 1);
  the audit promotes it to blocking because a verified misquote is exactly
  the credibility defect this gate exists to stop.

## Non-critical notes

- **Two Table 3 cells not traceable to the committed artifact** (off-target
  Peak RSS: geode-fem "3.1 GB", Palace "0.5 GB/rank", main.tex L1032–1035):
  `benchmarks/transmon_bench_cpu/results.toml` `[matched.off_target.*]`
  carries only wall times — no RSS keys — while the caption sources the
  matched values to those tables and the abstract promises every number
  traces to a committed artifact. Plausibly carried over from the
  physical-target rows (identical 133,108-DOF pencil), and no conflicting
  value exists anywhere (hence not a numerical inconsistency). Reviser
  options: commit the measured off-target RSS, footnote the carry-over, or
  drop the two cells.
- **"137×" rounding basis** (§8.3, main.tex L1124–1125): the paper computes
  137× from the displayed medians (4.388/0.032 — self-consistent as
  explicitly stated), but the artifact's own HONEST READ comment computes
  136× from unrounded values (4.388126/0.032302 = 135.9). Align to 136× (or
  keep, since the basis is stated); cosmetic.
- **Unverified citations (49)**: claim-support could not be verified because
  source PDFs are not in `<thread>/refs/`. The 7 newly-merged v5 keys plus
  xue2023jax's venue WERE verified live this audit (all support, one partial
  per the critical flag). Author should verify the remainder off-disk.
- **Uncitable architectural claim** (§1 L120–123): "no derivative path /
  none exposes an adjoint of its solve" for HFSS/COMSOL is uncited and (per
  the litsearch gap analysis) uncitable for closed-source products; it is
  framed as architecture, with a "to our knowledge" hedge on the wedge claim.
  Acceptable; noted for author awareness.
- **Five `TODO(operator)` markers** remain (title wording, affiliations,
  cubecl f64 tracking URL, repo URL/DOI, acknowledgment) — hard arXiv
  submission blockers but operator-owned inputs, not artifact-quality
  defects; already itemized by the reviewer.
- **Build: clean.** pdflatex/bibtex converged at 4 total passes (aux
  fixpoint; the fixed-2-post-bibtex-pass heuristic would NOT have sufficed —
  pass 3 still emitted the rerun warning), all exits 0, zero `[??]`/`??` in
  the rendered 25-page PDF. The shipped `main.pdf` is current (pdftotext
  stream byte-identical to the fresh compile; the v4 staleness defect stays
  fixed). Compile ran in a scratch copy to honor the do-not-modify-version-dir
  directive; see compile-log.txt header.
- **No stale figures**: all six figure scripts re-executed against the
  committed TOMLs; fig4/fig5 pixel-identical, the rest content-identical
  (font-rasterization deltas only).
