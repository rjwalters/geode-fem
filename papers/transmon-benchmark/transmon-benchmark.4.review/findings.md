# Findings — transmon-benchmark.4 (cross-section observations)

## Build verification (reviewer-run; render-gate fail-open)

The step-4b render gate was skipped fail-open: `pub-audit` has not produced a
`compile-log.txt` for the current post-#557 source state (stdout note:
"pub-review: render-gate skipped — main.pdf / compile-log.txt not present for
the current source state; run pub-audit first"). The reviewer instead compiled
the current `main.tex` independently in an isolated scratch copy:
`pdflatex → bibtex → pdflatex ×3` reaches a clean fixpoint — 24 pages, zero
undefined citations, zero undefined references, zero LaTeX errors. **No
build/compile critical flag.** Residuals: two overfull hboxes above the 5 pt
threshold (36.96 pt at source lines 991–1005; 17.75 pt at 1008–1020), both in
§10 (GPU cell), both pre-existing per the v4 changelog.

**However, the `main.pdf` shipped in the version dir is stale**: its mtime
(2026-07-14) predates the out-of-band PR #557 edit to `main.tex` (2026-07-16),
so the rendered artifact does not contain the corrected §8.2 scale paragraph.
Major finding (see comments.md); `pub-audit` must re-render before any share.

## The load-bearing cross-section defect: a half-applied correction

The single blocker-class observation of this review is structural, not local:
PR #557 corrected §8.2's large-scale story in place (the 1.16M-DOF OOM was "a
small-box truncation artifact"; given memory the direct solve completes at
565.5 s / 92.2 GB and "loses to Palace on both axes" — "a flop-and-fill
deficit, not merely a memory wall") but the correction was applied to §8.2
only. The abstract, the contributions bullet, Table 3's caption and rows, the
§8.2 trade-off closing paragraph, the Discussion performance summary, and
Limitations (iv) all still assert the retracted memory-wall/OOM causal story.
The paper currently argues against itself across sections — exactly the
defect class an in-place edit without a cross-section sweep produces. Raised
as the critical flag (see verdict.md); full site list in comments.md.

## Iteration context

- Prior review: `transmon-benchmark.3.review` — 42/44, `advance: true`, no
  flags, rubric `anvil-pub-v2`. This review: same rubric, so no rubric
  version transition subsection is required (steady-state case; omitted per
  step 10b).
- v4 was a directed number-migration revise (issue #536), not a critic-driven
  revise; the v3 dim-9 trim recommendations were therefore never applied and
  the deduction carries forward.
- v4's changelog declares "iteration 4 of max_iterations = 4 — the LAST
  allowed iteration", but `.anvil.json` now sets `max_iterations: 6`
  (operator-raised), so the v5 revise is NOT cap-blocked. The v4
  `_progress.json` metadata still reads `max_iterations: 4`; the orchestrator
  should reconcile.
- The superseding 2026-07-16 BRIEF reframe (differentiable transmon design,
  LOM branch) means v5 is a rewrite-scale revision regardless of this
  review's flag; the systematic reframe-delta list the reviser must apply is
  in comments.md §A, with resolver-verification caveats on the BRIEF's
  related-work identifiers in comments.md §B.

## Conditional tiers (all inactive)

- Corpus/provenance tier (#612): no `corpus:` declaration in the BRIEF —
  inactive; no `provenance_back_check` block emitted.
- Subject voice tier (#613): no `subjects` declaration — inactive.
- External-artifact verification (#663): no `artifact_verify` block in
  `.anvil.json` — not run.
- Venue overlay: `venue: "arxiv"` resolved to
  `.anvil/skills/pub/rubrics/arxiv.yaml` — advisory overlay scored 9/10; see
  `_review.venue.json`.

## Deterministic pre-scoring checks

- Numeric consistency (step 4c): ran with `--write-review`; sidecar
  `transmon-benchmark.4.numeric/_review.json` written (511 numbers, 0
  findings, pass).
- Evidence check (step 5b): ran against the resolved body; 9 dimensions,
  zero findings, pass.
- Citation cross-check: 50/50 `\cite` keys resolve; 0 unused `refs.bib`
  entries.
