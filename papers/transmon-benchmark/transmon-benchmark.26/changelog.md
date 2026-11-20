# Changelog: transmon-benchmark.25 → .26

Input: geode-fem#1488 (split from epic #569), the manuscript counterpart of `docs/research/geode-vs-palace-comparison.md` (#572). `.25` is not edited.

- New section "GEODE and Palace: Measured Performance and the Complement" (`sec:palace-compare`), placed before Discussion. It has one table (`tab:palace-wins`) of the three measured cases where Palace is ahead, each with its source file; a paragraph on the readings that lean toward geode-fem (one-core wall clock, core-seconds) with the caveats that keep them from being wins; the admission that no corner is clearly preferred on raw solve performance; and a paragraph on the solver-derived design sensitivities, with tolerances taken from the committed tests and `benchmarks/transmon_diffopt/*.toml`.
- Evidence-tiers table, *Not claimed* row: "a speed advantage over Palace" now cross-references the new section. No tier row was added.
- The one "faster" in the section ("not offered as a faster replacement") is a negation of the claim, consistent with the *Not claimed* row. The words "outperform" and "wins" are used only for Palace or in negation.

No other text, number, figure, reference or author field changed; no computation run. `refs.bib` and `figures/` are copied unchanged (the section cites no new work).

Not built here: this host has no TeX toolchain, so `main.pdf` is absent and `papers/build.sh` was not run (as for `.25`). Build before any upload.

Checks run: `verify_evidence.py` passes (its pin of the *Not claimed* row now includes the cross-reference). `verify_figures.py` fails on this host for `.25` and `.26` alike ("regenerated figure bytes differ"; this host's Matplotlib differs from 3.10.8); no figure changed, and the figures are byte-identical to `.25`.
