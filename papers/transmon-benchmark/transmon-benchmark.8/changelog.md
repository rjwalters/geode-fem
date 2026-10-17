# transmon-benchmark.7 → transmon-benchmark.8

Prepared for #1059 on PR #1060, 2026-10-11. An operator-requested figure
revision of the merged v7 (`main` at `0951f862`, merge `db32b4f4`, PR #1040),
plus the non-blocking carry-overs of the v8 tracker #1063. Revising past the
old cap is covered by the operator's standing permission recorded on PR #1040
("we can continue revising the paper as many times as we want"); there is no
iteration gate. No review, audit or score is claimed or carried forward. v8 is
a DRAFT. v1–v7 and every critic sibling are byte-identical to `main`.

**v8 = final v7 + figure revision.** The manuscript text is v7's. Nothing in
v7's claims, numbers, scope statements or caveats is changed, except the GPU
wording below. The changes are figures, captions, figure placement, a short
appendix, and two added figure references.

## How v8 was rebuilt (and what an earlier v8 draft got wrong)

The first v8 draft on PR #1060 copied v7 at `a50a446a`, an intermediate state.
v7 then had two more Doctor passes and three Judge rounds before it merged, so
that draft lacked the #1057 convergence study (Table 3, the convergence
figure), the final §9.4 with the #1058 seven-round held-out series, the
scalar-versus-tensor scope, the native-f64 / outside-the-Burn-tape disclosure,
the #1037 "ruled 2026-10-10, not yet built" status, the regenerated historical
panel and the strengthened verifier. It had also pinned the #1058 evidence at
`1b445770`, before the seven-round series, and restated the withdrawn
single-round "1 %" claim.

v8 was therefore rebuilt from final v7: `main.tex` starts from `main`'s
`transmon-benchmark.7/main.tex`, and only the draft's figure work was ported
(the diff `a50a446a` v7 → draft v8 isolated it). Removed or rescoped from that
draft:

| Draft content | Disposition in v8 |
|---|---|
| New §9 subsection "Refined P2 optimization and fixed-design validation": one held-out refinement (89.662755 fF, 0.2645 %), "These results meet the working 1 % local capacitance/sensitivity check", sensitivities "change by at most 0.605 %", Palace "0.000249 %" | **Removed.** v7's §9.4 is the text: seven held-out rounds, a rigorous upper bound (below 89.02 fF), an estimate (about 88.8–88.9 fF), "We do not claim the 1 % threshold is met". |
| Draft Figure 5(b) "Fixed-design checks" with ±1 % "working tolerance" shading | **Replaced** by Figure 6(b): the seven-round series of both designs from the committed `summary.toml` files, bounds, estimate bands (labelled not bounds), slow-rate fits, and the by-construction label. |
| Abstract, conclusion and limitations sentences: "passes a held-out local refinement check", "local refinement stability", "passes local checks" | **Removed.** v7's abstract, conclusion and limitations stand. |
| "Pending" paragraph with `[PENDING issue-1051]` / `[PENDING issue-1037]` and text "being measured" | **Removed.** #1051 is reported (v7 §7); #1037 is v7's roadmap paragraph ("ruled and not yet built" in the sidecars). No pending marker. |
| §9 intro and §4.1 mesh sentence rewritten around the draft subsection | **Dropped**; v7's versions stand. |
| Appendix link to the PR #1058 tree at `1b445770` (a GitHub URL; not on `main`) | **Removed.** The repository URL is an operator item; paths are given as `\path`. |
| Three JSON snapshots from `1b445770` (`refined_results.json`, `refined_validation.json` with `"passed"` verdict fields, `refined_design.json`) | **Deleted.** Figure 6 reads the committed artifacts. The derived outline snapshot is kept (see `provenance.md`). |
| Draft Figure 7 (legacy) built from the `a50a446a` script: panel (b) titled "Historical one-parameter run / Limited by mesh validity" | **Rebuilt** from final v7's script: "Historical, coarse 133k-tet mesh: anchor shortfall was a mesh artifact". Its caption is final v7's, verbatim. |

## Figures

The file number is now the printed number (this also closes the v6 review
nit on file/number skew). Every figure is drawn at most 6.3 in wide and
included at 0.98 of the text width, so no font is below 8 pt on the page; the
shared `figure_style.py` refuses to save a figure with smaller text, text off
the canvas, a legend over text, or colliding annotations.

| v8 | v7 | Change |
|---|---|---|
| Fig. 1 `fig1-geometry` | Fig. 1 | Redrawn (draft work, kept): conductor outlines, junction-sheet detail, schematic cross-section. Caption rewritten for the three-conductor split. |
| Fig. 2 `fig2-agreement` | Fig. 2 | Redrawn (draft work, checked): adds the confirmed 3.45 GHz qubit mode; signed differences; no "Same-mesh" and no `junction_lc` label (the review findings); "box/cavity" mode names as in Table 1. Caption rewritten; Figure 2 is now referenced in §5. |
| Fig. 3 `fig3-lscaling` | Fig. 3 | Redrawn: separate qubit and port-sheet panels, committed ratios, linear axes. Caption: v7's numbers, "log-log" dropped, panel structure added. |
| Fig. 4 `fig4-participation` | Fig. 4 | Redrawn: separate axes for p and port EPR; direct labels (the draft's legend covered the 17.49 GHz stem); v7's Palace mode overlay restored as a rug. |
| Fig. 5 `fig5-convergence` | Fig. 5a | Restyled; adds the tensor-ε P1 series and its bound. v7's panel (b) moved to Fig. 6b. |
| Fig. 6 `fig6-refined-optimization` | — | **New.** Optimizer steps of both runs, the seven-round held-out series with bounds, bands and the by-construction label, and the actual island outlines. Placed in §9.4 (`[tbp]`; it is taller than a top float). |
| Fig. 7 `fig7-cpu-wallclock` | Fig. 7 | Redrawn (draft work): one linear axis, medians and min/max; "six reference modes" (not "physical"); † as in Table 6. Caption rewritten; Figure 7 is now referenced in §11.1. |
| Fig. 8 `fig8-legacy-diffopt` | Fig. 6 | Moved to new Appendix A with v7's caption; legend no longer covers the "first tet inversion" label (#1063). |

Text edits that follow from the figure move: §9.1 and §9.2 point to Figure 8,
§9.4 points to Figure 6a, 6b and 6c–e, §7 to Figure 5. A one-page Appendix A
says why Figure 8 is historical and where the one outline snapshot comes from.

## #1063 carry-overs (non-blocking)

| Item | Change |
|---|---|
| `verify_evidence.py` does not assert the title | It now asserts the title block, with its `TODO(operator)` comment, is identical to v6 and v7. |
| `provenance.md` "constant to within 8.1e-5" | Now "a relative spread of 8.1e-5 (largest ratio over smallest, minus one)", as the manuscript says. |
| "slower than the CPU at every measured size" (§8.1) and "loses to every CPU configuration at every measured size" (§11.3, bold) | At 25,695 edges GPU matrix-free (81.764 s) is not slower than CPU matrix-free (82.056 s). Both places now say: slower than both assembled CPU configurations at every size, and slower than CPU matrix-free at every size except the largest, where the two are at parity. The verifier checks each inequality against the 16 committed cells. |
| Figure 6(b) legend overlaps the "first tet inversion" label | Fixed in its v8 home, Figure 8(b). |

**Not resolved, by design (operator items of #1063):** the title; what the
89.9 fF anchor means relative to the junction's 5.5 fF; the "first independent
validation" claim; the acknowledgments and the other `TODO(operator)` lines.
All six `TODO(operator)` lines are unchanged.

The citation set, `refs.bib` and `main.bbl` are unchanged. The PDF is 38 pages
(v7: 36): one new figure, one appendix page.
