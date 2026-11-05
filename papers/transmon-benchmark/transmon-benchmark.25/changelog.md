# Changelog: transmon-benchmark.24 → .25

Input: geode-fem#1231, the v2 list after the Zenodo v1 release (transmon items, Judge nits on #1230). `.24` is the published text and is not edited.

- Figure 4 legend: "P1" and "P2" are now "p = 1" and "p = 2", as in Figure 2 and the text. The "(b) upper bound per pass" key reads "(p = 2)".
- Figure 4(b) title: "gap 2.5–3.1%" read like the gap between the upper and lower bounds. It now reads "tensor − scalar 2.5–3.1%", the bounded tensor-minus-scalar difference as a share of the scalar value (as in the text of the Figure 4 discussion). The caption says the same, and "conforming P2 value" is now "conforming $p = 2$ value". The values are unchanged. Only `figures/src/fig4_convergence.py` and `figures/fig4-convergence.pdf` changed in `figures/`; the PDF was regenerated with Matplotlib 3.10.8, the version that produced the other committed figures (the unmodified script reproduced the committed bytes first).
- `verify_figures.py`: asserts the new Figure 4 strings; Figure 4 is no longer pinned to v19's bytes. Its Figure 2 key checks (`p = 1`, `r = 1`, `h → 0`) now match the text as drawn since v20; they failed on `.24`.
- `verify_evidence.py`: the author-block pins accept the `.24` corresponding-author footnote (the "no email" ruling of 2026-10-22 is relaxed for that address only); the one-time v22-to-v23 comparison runs only in the `.23` source.

Already in `.24`, so not changed: the precision argument is already qualified ("On the levels both codes ran, the two coarsest family-a levels with linear elements"), and the Figure 1 caption already exempts the $k$, Vacuum, Sapphire and Metal-sheets labels (v23).

No number, claim, title, author, affiliation, reference or DOI changed; no computation run.

Not built here: this host has no TeX toolchain, so `main.pdf` is absent from this directory. Build with `papers/build.sh` before the Zenodo re-upload.
