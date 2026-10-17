# v8 author-side checks — 2026-10-11 (PR #1060)

These are author-side checks of transcription, figure generation and
packaging. They are not an independent review, audit or numeric critic, and
no score is claimed. No geode or Palace solve and no benchmark generator was
run: v8 consumes artifacts already committed on `main`. v7's own check
history is in `../transmon-benchmark.7/validation.md`; v8 inherits v7's text,
so v7's pass-4 checks of that text still describe it, and every v7 verifier
assertion is re-run here.

Environment: Python 3.14 with NumPy 2.4.1 and Matplotlib 3.10.8; Poppler
(`pdftotext`, `pdfinfo`, `pdftoppm`); TeX Live / pdfTeX. `main` at
`f285e969` merged into the branch.

- **Rebuilt from final v7.** `main.tex` was started from `main`'s
  `transmon-benchmark.7/main.tex`. The figure work of the earlier v8 draft was
  ported from the diff `a50a446a` v7 → draft v8; nothing else from that draft
  was kept (see `changelog.md`). `diff v7/main.tex v8/main.tex` shows only the
  header comment, the `\anvilfig` default width, figure environments and
  captions, figure references, the two GPU sentences, and Appendix A.
- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.8`: PASS,
  three pdflatex passes, **38 pages** (v7: 36). `main.log` has no undefined
  reference or citation, no overfull box and no LaTeX error; the extracted
  text has no `??`. `main.bbl` is byte-identical to v7's (citation set
  unchanged), and so is `refs.bib`.
- `python3.14 verify_evidence.py`: PASS. It is final v7's verifier with every
  per-occurrence pin kept. Six counts moved, each because a v8 caption adds a
  copy: 89.50 fF (Figure 5), 89.84–89.96 fF, 88.77–88.91 fF, 448,279 and
  90.978253 (Figure 6), "about 17 %" (Figure 7); the script names each. The
  by-construction copy that v7 had in its Figure 5 caption is now checked in
  the Figure 6 caption; the count stays six. New: the title block, with its
  `TODO(operator)` comment, is asserted identical to v6 and v7 (#1063); the
  six `TODO(operator)` lines are asserted identical to v6 and v7; the two
  #1063 operator-item sentences on "first independent validation" are asserted
  unchanged; the GPU wording is checked against all 16 cells of
  `gpu_driven_scaling/results.toml` (GPU slower than both assembled CPU
  configurations at every size, and slower than CPU matrix-free at every size
  except 25,695 edges); and the withdrawn single-round wording of the earlier
  draft ("meet the working 1 %", "0.2645", "89.6628", "fixed-design", ...) is
  asserted absent. Also PASS under Python 3.11.
- `python3.14 verify_figures.py`: PASS. Input SHA-256s from
  `figures/src/data/provenance.json` (nine committed artifacts and the one
  snapshot); the outline snapshot's bounding box equals the committed
  `p2_cycles10/design.json` exactly; all eight scripts run and every PDF
  regenerates byte-for-byte; every PDF is at most 6.3 in wide; figure content
  from the extracted text (Figure 2 has the qubit mode at +0.00005 % and its six
  signed values reproduce Table 1's `rel_err_pct`; no figure says "Same-mesh",
  `junction_lc`, "physical modes", "passes", "±1 %" or "working tolerance";
  Figure 6 shows rounds 0–7 of both designs, `< 89.02` and `< 90.08`, the
  estimate and slow-rate markers, and "contains the anchor by construction";
  Figure 8 says "anchor shortfall was a mesh artifact"); and `main.tex`
  includes each figure once, in printed order.
- **Layout rules, enforced in code.** `figure_style.save()` refuses a figure
  wider than 6.3 in, any text under 8 pt, text off the canvas, a legend over
  any text, and an annotation over another annotation, tick label, axis label
  or title. Figures are included at 0.98 × 6.5 in, so the smallest printed
  font is at least 8.09 pt. While the figures were redrawn this check rejected
  sixteen drafts for text collisions or text off the canvas (among them a
  legend over a data label in Figures 5 and 6, and a Figure 8(b) legend over
  "first tet inversion", the #1063 finding).
  It looks at text boxes only, so text over a data line was checked by eye.
- **Mutation check of both verifiers:** 26 single edits to a temporary copy
  of the version directory, each of which makes a verifier fail: the title;
  both GPU sentences restored to the v7 wording and one GPU number; the Figure 6
  caption's corrected bound, band edge, tetrahedron count, target, deleted
  by-construction sentence and deleted "No lower bound is proven"; the Figure 5
  tensor bound; the Figure 7 "17 %"; the withdrawn "meet the working 1 %"
  sentence and "0.2645 %" shift reinserted; a `[PENDING]` marker; a
  `TODO(operator)` line; the "first independent validation" sentence; an
  inherited abstract bound and scalar-scope phrase; figure order; a stale
  `fig:diffopt` reference; one outline-snapshot coordinate; stale PDF bytes;
  the Figure 6 by-construction label in the figure script; a Figure 4 label
  moved onto another; and the Figure 8 legend moved back over its label. The
  figure-script mutations are also caught by the byte comparison; the layout
  check's own effect is the list of rejected collisions above.
- **Visual inspection** of every figure at print size in the built PDF:
  Figure 1 (p. 8), 2 (p. 10), 3 (p. 11), 4 (p. 14), 5 (p. 16), 6 (p. 22), 7
  (p. 26) and 8 (p. 33, Appendix A). Figure 6 is placed `[tbp]` because it is
  taller than LaTeX's top-float limit; as `[t]` it drifted to p. 31. A
  `\clearpage` keeps Figure 8 out of the bibliography. Also rendered and
  inspected as standalone PNGs while drawing.
- **Standalone package check:** `main.tex`, `main.bbl`, `refs.bib`,
  `anvil-paper.cls` and `figures/` copied to an empty directory, `TEXINPUTS`
  unset, plain pdflatex three times without BibTeX: PASS, 38 pages, no
  undefined reference, no overfull box, no `??`.
- **Anvil pending-marker check** (`anvil.lib.pending_marker --write-review`):
  **exit 0**, no marker; `.8.pending/_review.json` regenerated by the tool.
  The earlier draft's two markers came from v7 at `a50a446a` and are gone with
  it. This removes the pending gate only; v8 is still a DRAFT.
- **Immutability:** `git diff origin/main` over `transmon-benchmark.0` through
  `.7` and every critic sibling (`.litsearch`, `.audit`, `.numeric`,
  `.review`, `.7.pending`) is empty.

Not checked: the inherited text was not re-audited beyond the verifier; no
generator was re-run, so the optimized-outline snapshot is checked against the
committed `design.json` but its source mesh was not regenerated; no Anvil
critic was run. Remaining: the #1063 operator decisions (title, anchor
meaning, "first independent validation", acknowledgments and the other
`TODO(operator)` lines), the two-sided bound (#1062), the two-target
experiment (#1037), the port formulation and artifact re-baselining
(#1044, #1045), and the Anvil review, audit and numeric pass before any
Zenodo deposit.
