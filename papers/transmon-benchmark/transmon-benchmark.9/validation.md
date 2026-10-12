# v9 author-side checks — 2026-10-12 (tracker #1063)

These are author-side checks of transcription, figure generation and
packaging. They are not an independent review, audit or numeric critic, and
no score is claimed. No geode or Palace solve and no benchmark generator was
run, and no cloud resource was used: v9 consumes artifacts already committed on
`main` (`9081d1f6`). v8's checks are in `../transmon-benchmark.8/validation.md`.

Environment: Python 3.14 with NumPy 2.4.1 and Matplotlib 3.10.8 (the
evidence verifier also under Python 3.11); Poppler (`pdftotext`, `pdfinfo`,
`pdftoppm`); TeX Live / pdfTeX.

- **Started from v8.** `main.tex` is `main`'s `transmon-benchmark.8/main.tex`,
  revised as listed in `changelog.md`. Every number is read from a committed
  artifact; the few derived values (ratios, percentages against the new anchor,
  the 49.6 % E_C consequence, the 1.1e-4 by-construction spread) are
  recomputed by the verifier and labelled in the text where they are ours.
- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.9`: PASS,
  three pdflatex passes, **43 pages** (v8: 38). `main.log` has no undefined
  reference or citation, no overfull box and no LaTeX error or float warning;
  the extracted text has no `??`. `main.bbl` and `refs.bib` are byte-identical
  to v8's.
- **Standalone package check:** `main.tex`, `main.bbl`, `refs.bib`,
  `anvil-paper.cls` and `figures/` in an empty directory, `TEXINPUTS` unset,
  plain pdflatex three times: PASS, 43 pages, no undefined reference, no
  overfull box, no `??`.
- `python3.14 verify_evidence.py`: PASS (also Python 3.11). Rebuilt against the
  current artifacts: v8's verifier fails on `main` (`generator_peak_rss_gb`,
  now [12.8, 14.4] after #1061). It pins the re-baselined keys (#1045), the A2
  measurements (#1044), the re-anchored LOM chain and anchor derivation (#1066),
  the uniform study with every anchor comparison re-based, the certified X⁰
  bracket and that all four #1057 estimates lie outside it (#1062), both current
  runs row by row with their bounds and estimates recomputed, every round's
  upper bound against the bracket artifact and the family brackets re-derived
  with their rounding (#1075), the by-construction spread of the new pair, the
  coupling and loop-inductance measurements, rules, anchors, same-mesh
  alternatives and every verdict-table cell recomputed from the unrounded
  brackets (#1078, #1079), the #1043 offset decomposition, the
  superseded-anchor headers of six diffopt artifacts, pinned occurrence counts
  of 18 headline values, a whitelist of every `[a, b]` bracket and every
  "below N fF" in the text, and the absence of v8's withdrawn statements
  (no lower bound, 89.02/90.08 fF, 8.1e-5, "has not been done", ...). Margins
  are floored: the corrected design is "at least 0.669 fF" above the
  loop-corrected band (artifact 0.6699), not 0.67.
- **Operator items (#1063):** the title block, the six inherited
  `TODO(operator)` lines, both "first independent validation" sentences and the
  acknowledgments paragraph are asserted identical to v6, v7 and v8. The v9
  draft's one new `TODO(operator)` line is resolved by the 2026-10-12 operator
  ruling: exactly the six inherited lines remain, and the replacement
  paragraph ("Which target the headline uses") is pinned.
- `python3.14 verify_figures.py`: PASS. Input SHA-256s of fourteen committed
  artifacts (no snapshot; the data directory holds only `provenance.json`, and
  every artifact path a script names must be in it); all nine scripts run and
  every PDF regenerates byte-for-byte; each PDF at most 6.3 in wide; figure
  content against the artifacts (Figure 3's committed/computed wording, Figure
  5's anchor and X⁰ bracket, Figure 6's four-step runs, brackets, estimate
  labels, by-construction label and committed extents, Figure 7's three targets,
  shifts, estimate bands and certified brackets, Figure 9's "superseded");
  inclusion order; caption wording each figure depends on, including Figure 8's
  "six reference modes" (a v8 review nit).
- **Layout rules, enforced in code** (`figure_style.save()`): width, fonts
  ≥ 8 pt at print size, nothing off the canvas, no legend or annotation over
  text. While drawing it rejected ten drafts (Figure 9: the note off the canvas
  four times and the legend over "first tet inversion"; Figure 6's panel title;
  Figure 7's bracket label, tick label and legend off the canvas; Figure 3's
  footer). It checks text boxes only; text over data lines and the Figure 6
  legend over a series were caught by eye and fixed.
- **Mutation check:** 38 single edits to a temporary copy of the version
  directory (35) or to a committed artifact restored afterwards (3); **all 38
  caught**. Text: both brackets (one copy each), the loop target, the
  "estimates" label, the "extrapolation, not a bound" for k, a same-mesh
  alternative, a verdict cell, the floored 0.669 margin, the by-construction
  spread, the Figure 6 by-construction caption sentence, the RSS range, the
  title, an inherited `TODO(operator)`, the new `TODO(operator)`, the
  validation claim, the acknowledgments, a stale E_C, the 17.3 %, a held-out
  row, the FD error rounded down, the no-Palace-P3 sentence, a `[PENDING]`
  marker, the re-baselining paragraph reverted, figure order, Figure 7's
  "not bounds", Figure 8's "six reference modes", the 94 % loop share, the L_s
  central value, the 1 % drift called a bound, the superseded-runs label, the A2
  residual. Figures: two script edits, stale PDF bytes, a provenance hash.
  Artifacts: the loop anchor value, the design bracket's lower bound, the
  central k.
- **Visual inspection** of every figure page in the built PDF: Figure 1
  (p. 8), 2 (p. 9), 3 (p. 11), 4 (p. 15), 5 (p. 17), 6 (p. 23), 7 (p. 27,
  §9.5), 8 (p. 30) and 9 (p. 38, Appendix A); also standalone PNGs while
  drawing.
- **Anvil pending-marker check** (`anvil.lib.pending_marker --write-review`):
  **exit 0**, no marker; `.9.pending/_review.json` written by the tool, same
  shape as v8's.
- **Immutability:** `git diff --stat main -- papers/` touches only
  `transmon-benchmark.9/`, `transmon-benchmark.9.pending/`, `BRIEF.md` and
  `papers/README.md`.

Not checked: no generator was re-run; the certified lower bounds, the coupling
fits and the loop-inductance extrapolations are transcribed from their
artifacts, not recomputed; no Anvil critic was run.

## Doctor pass on PR #1082 (2026-10-12)

The operator asked for the Judge's four non-blocking nits and the §9.5
`TODO(operator)` ruling to be applied before merge (#1063, comment
6131068921). Checked after the edits:

- `python3.14 verify_evidence.py`: PASS. New pins: the k/g ratios (28 % for k,
  34 % = 61.7033/46.0242 for g, recomputed from the artifact; the old pairing
  is asserted absent); the increment [15.15, 15.59] recomputed from
  `max(best_lower)` of the bracket rows and the series' ceiled X⁰ round 7, and
  the artifact's [15.14, 15.59] recomputed from its `x0_family_upper_ff`;
  "on the optimization mesh" in the abstract; exactly the six inherited
  `TODO(operator)` lines and the pinned replacement paragraph; pin counts for
  93.954 fF (8) and 93.143 fF (9).
- `python3.14 verify_figures.py`: PASS, byte-identical regeneration of all
  nine PDFs; Figure 5 is checked for the inset title "certified bracket
  (0.21 fF wide)", the inset's two end ticks and "magnified in the inset" in
  the footer and caption.
- **Mutation check:** 12 single edits, each reverted, all caught: the
  increment to [15.16, …] and back to [15.14, …] in the quoted sentence; g's
  34 % to 33 %; the 28 % removed from k; "on the optimization mesh" removed;
  "not scheduled" to "scheduled"; "estimate" to "target"; the old
  `TODO(operator)` line re-inserted; 77.607978 to 77.607977; the inset title
  edited and regenerated; the fig5 script edited without regenerating; the
  caption's "magnified in the inset" reworded.
- **Build:** 43 pages, 0 undefined references or citations, 0 overfull boxes
  (one first draft overflowed on the code names in the new paragraph; fixed
  with `\path`). Figure 5 stays on p. 17 (a first, taller draft floated it to
  p. 35 and was shrunk). Page 17 inspected as a PNG, and Figure 5 as a
  standalone PNG; the changed text on pages 1, 25 and 26 checked in the
  extracted PDF text.
- **Anvil pending-marker check:** exit 0; `.9.pending/_review.json`
  regenerated by the tool (unchanged bytes).
