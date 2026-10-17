# Findings — transmon-benchmark.6 (cross-section observations)

## Surgical-diff integrity (iteration 6 of 6)

The load-bearing question for a 6-of-6 surgical revision is whether the
diff is exactly what the changelog declares. It is:

- `diff transmon-benchmark.5/main.tex transmon-benchmark.6/main.tex`
  contains five hunks: (1) the header provenance comment (v6 header added,
  v5 header retained under a "---- v5 provenance note (historical)"
  marker); (2) §1 L127–128 unquoting the `eriksson2025automated` span;
  (3) §9.2 L916–919 the 0.2155/0.2156 anchor clarifier; (4) Table 3
  L1049–1052 ddag markers + L1073–1078 the provenance footnote; (5) §8.3
  L1145–1150 the 137×→136× alignment. Nothing else.
- `diff -r --brief` across the version dirs: only `main.tex`,
  `changelog.md`, `_progress.json`, and the recompiled `main.pdf` differ.
  `refs.bib`, `anvil-paper.cls`, `main.bbl`, and all of `figures/` are
  byte-identical to v5.
- The reviser's self-reported inserted-then-reverted "placeholder-sentinel"
  string is confirmed absent: the only "placeholder" occurrences in the
  body are the pre-existing figure-placeholder macro (L57–64), unchanged
  from v5. No stray sentinel, TODO, XXX, or FIXME tokens were introduced.
- `TODO(operator)` count is unchanged (6 grep lines = the same five
  operator items as v5).

## Artifact cross-checks performed this pass

- `benchmarks/transmon_bench_cpu/results.toml`: `[matched.off_target.*]`
  carries `wall_s` only (36.8/26.6/248.0/64.7 — matching Table 3);
  `[matched.physical_target.*]` carries `peak_rss_gb = 3.1` /
  `peak_rss_gb_per_rank = 0.5` at `n_interior_dofs = 133108` — the new
  footnote's wording is exactly right, including "same session" (the
  header declares the matched tables same-box, same-session).
- `benchmarks/gpu_driven_scaling/results.toml`: HONEST READ comment states
  "136x faster at n=6 (0.032 s vs 4.39 s) and still 44x faster at n=15";
  unrounded medians `4.388126 / 0.032302 = 135.86 → 136×`. The paper's new
  sentence ("Computed from the artifact's unrounded medians ... 136×
  faster at 1,854 edges (displayed: 0.032 versus 4.39 s)") is faithful;
  companion 44× and 13.5× (81.8/1.86 = 44.0; 81.8/6.04 = 13.54) hold.
- `benchmarks/transmon_diffopt/pad_results.toml` `[anchor_attempt]`:
  `c_sigma_target_ff = 89.9`, `e_c_target_ghz = 0.215464` (→ 0.2155);
  `benchmarks/transmon_diffopt/results.toml`: `c_target_ff = 89.843364`,
  `e_c_target_ghz = 0.215600` (→ 0.2156). The §9.2 clarifier's rounded-vs-
  back-solved explanation is exactly the artifact relationship.
- The unquoted §1 paraphrase remains substance-supported by the QDO
  abstract per the v5 audit's first-hand arXiv:2508.18027 verification
  ("time-consuming iterative electromagnetic simulations requiring manual
  intervention"; HFSS + separate physics-informed guiding models);
  "iterates" preserves the dropped word's meaning without asserting
  verbatim source words.

## Build and deterministic checks

- Fresh scratch-copy compile to fixpoint: pdflatex ×4 + bibtex (pass 3
  still emitted "Label(s) may have changed"; pass 4 clean — the same
  4-pass fixpoint the v5 audit recorded). Zero undefined
  citations/references; one 1.13 pt overfull hbox (below the 5 pt gate);
  25 pages. Shipped `main.pdf` pdftotext-identical to the fresh compile
  (not stale).
- Numeric-consistency detector (`anvil.lib.numeric_consistency`,
  `--write-review`): 578 numbers extracted, 0 claims flagged, pass;
  sidecar at `transmon-benchmark.6.numeric/_review.json`.
- Citation cross-check: 57 keys cited in `main.aux`, 57 entries in
  `refs.bib`, zero missing in either direction; clean bibtex pass.
- Evidence self-check (`anvil.lib.evidence_check`) on this review's
  `scoring.md`: 9 dimensions checked, 0 findings.

## Residuals for pub-audit

The v5 audit's non-critical notes are addressed (RSS footnote, 136×) or
carried by convention (TODO(operator), 49 off-disk citations, hedged
HFSS/COMSOL architectural claim). The v6 audit pass should re-run the
citation-audit on the changed §1 site (now paraphrase, not quotation) and
confirm the flag closure formally; everything else in the diff is
artifact-verified above.

(No rubric version transition: the prior review at
`transmon-benchmark.5.review/` was scored against the same `anvil-pub-v2`
rubric.)
