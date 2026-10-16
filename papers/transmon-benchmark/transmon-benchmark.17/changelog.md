# transmon-benchmark.17 changelog

Revision of v16 against three independent reviews of v16 held outside the thread
(no `.16.review/` sibling): Opus (41/44), Fable (40/44) and Codex (no score). It
also adds two results merged on `main` after v16: the grounded-box upper bound on
$C_{\rm isl}$ (T6) and Palace's adaptive run on the published final design (T7).
No solver and no `cargo` was run for v17; text, tables, bibliography, captions and
verifier changes only. Figures 1–5 are v16's bytes. The operator's rulings stand
(title, derivative-scope paragraphs, short Section 8, "bounded by [a, b]" with the
tiers Bounded / Model-dependent / Estimated, the approved acknowledgment and
provenance section).

## Must-fixes

| Source | Note | Resolution |
|---|---|---|
| Opus MF-1, Codex M1 | Box-model split not carried through §9, Table 5, abstract, contribution 2 | T6 gives the grounded box its own upper bound, so the eigenproblem's model now has a two-sided bracket: $C_{\rm isl}$ in [79.56, 79.77] fF, $C_\Sigma$ in [85.06, 85.27] fF. §6 "Two box models" states the monotonicity transfer (zero or floating constant on the box) and that the upper bound is re-solved with the box grounded; "The unmodified device" gives the grounded bracket, the same-mesh difference (+0.0065 fF, $8\times10^{-5}$, an estimate) and its bound [0, 0.21] fF, and notes that a floating box is covered. The static route is read from the grounded bound ([4.449, 4.458] GHz; the natural bound moves the low end by 0.3 MHz). §9 and Table 6 use the grounded $C_\Sigma$: $E_C/h$ in [0.2271, 0.2278] GHz, two-node published 15.6–16.1 % above, bare 16.6–17.0 % above (was "at least 16.6 %"; now two-sided). Table 5 Bounded lists both boxes; contribution 2 gives both brackets; the abstract says "for the grounded box". The operator's "about 1e-5 relative" is 8.1e-5 in the artifact; the text prints $8\times10^{-5}$. Codex's 85.06–85.26 fF is $C_\Sigma$ (with $C_J$); the paper's $C_{\rm isl}$ is 79.56–79.76/79.77 fF. |
| Fable M1 | §9 percentages change referent; "about half" | Rewritten so every percentage names its comparison (published over bound, over static route, over adaptive level) and Table 6 holds the values. The adaptive reading is 14.48 % (printed 14.5 %, was "about 14 %"). The requested "more than half (1.5–1.9 of 2.6 points)" holds only against the rounded 14 %: unrounded, the adaptive-to-static gap is 1.0–1.4 of 2.2 points, between "about half" and "more than half" at its two ends. The text therefore gives the factors (1.166–1.169 = 1.145 × a further 0.9–1.3 % × the remaining 0.6–1.2 % for $L_s$ and $k$) and no fraction word. |
| Fable M2 | Abstract: extrapolations are Palace-only | Abstract: "these estimates lean low and rest on Palace's levels alone, since geode-fem ran only order 1 on the two coarsest meshes"; contribution 1 and §1 say the same. |
| T7 | Adaptive run on the final geometry | §5 "Adaptive refinement" covers both geometries: final from 1.17 to 14.6 M unknowns, 3.0164 → 3.2468 GHz, +8.0 % over the published 3.005 GHz, measured and not extrapolated; third solve exceeds the highest uniform level (3.1884 vs 3.1464); inside the band, 0.56 % and 1.39 % below the family estimates; stopped on the $10^7$ size limit, not tolerance, last step +0.63 %; fit 3.2672 GHz (α = 0.83), 16 MHz per +0.1 %, an ill-conditioned estimate; resonator fit unreliable. Table 2 gains an "Adaptive, finest level" column (a measured value); Table 5 Measured, the abstract, §1 and the conclusion carry the +8.0 %; the reproducibility section gives the 24 ranks. |
| Companion pointer | "still being run" | §9: "A companion paper on adjoint shape optimization of a conformal patch antenna … at 38 shape parameters the adjoint needs at least 11 times fewer solves than the best gradient-free run, tuned settings included, to reach its −9.5 dB target, the ratio falls to 1.1–2.4 at 4 parameters, and a tuned full-covariance CMA-ES comes within a factor of 7.7 at −9 dB." The brief's "about 1–2×" is 1.1× (−9 dB) and 2.4× (−9.5 dB) in the tuned README, so the measured range is printed. Bibliography: generic working title, "Preprint in preparation, to be deposited on Zenodo". **These counts come from PR #1150 (`feature/issue-1147`), open when v17 was drafted.** |

## Should-fixes

| Source | Resolution |
|---|---|
| Opus SF-1 | Practitioner paragraph points to Ye et al.'s gap-refined fourth-order setting (Section 2). |
| Opus SF-2 | Threats: sheet-outline edges are mostly 10–12 µm on $h$ (median 11.9 µm, from DeviceLayout's mesh by `sheet_edge_lengths.py`), 2.5–3 µm on $h/4$, more than ten times a 0.2 µm film. |
| Opus SF-3 | E_C paragraph → Table 6; the "1.5 % below Broyden" repeat is cut from §9 (points back to §7); Table 5 no longer repeats the two-family ranges. |
| Fable S1 | Extrapolation rule itemized, with the initial qubit's six values. |
| Fable S3 | Abstract "2×10⁻⁴ %" covers every shared mesh; §5 gives 10⁻⁴ % for the cross-check ladders, §7 2×10⁻⁴ % for its ladders. |
| Fable S4 | §5 "Result" gives both families: 6.2–6.8 % and 7.6–8.4 % low on the qubits, 2.2–2.8 % on the resonators. |
| Fable S5 | Practitioner paragraph: 993 of 1,007 bisection edges lie where a resonator or feedline strip is one element wide (on the fixture). |
| Fable S7 | Repetition cut: "consistency check" kept in §5 and Table 5 (§6 points back); "lean low" in §6/§5 trimmed; the §5 bisection mechanism points to §4; the §1 numbers that §4 repeats are cut. |
| Fable S8, Codex | Static route now from the grounded bound; Fig. 4 caption "All values are for the natural box"; Fig. 2 caption "(grounded box)". |
| Codex | Sign convention: "for the exact field $D = \varepsilon\nabla\phi$, the negative of the physical displacement, they are the conductor charges". δ calibration: "the geode-fem solves of the one-time δ calibration are outside the Palace-run count". |
| Opus N-2 | GCI with numbers: fine-grid index, $F_s$ = 1.25, centred on 4.155 GHz, half-width 0.65 GHz, reaching 4.81 GHz. |
| Opus N-3 | The 6×10⁻⁵ % shunt shift is stated in §3.3 and referenced from §5. |
| Opus N-4 | Contribution 4: "the isotropic sheet term of the port model". |
| Opus N-5 | Box paragraph: "to zero or to a floating constant". |
| Fable N1 | "which gives a larger $C_\Sigma$ than the two-node one for every frequency". |
| Fable N2 | `burn` year 2026 (v0.21.0 released 2026-05-07, GitHub release API). |
| Fable N3 | "the rule used before" removed. |
| Fable N4 | $L_s$, $k$ "fitted". |
| Fable N6 | Abstract "in our environment". |
| Fable N9 | Heuristic sentence trimmed. |
| Length | Footnote on binary64 folded into the text; Figure 5, 2, 3 and Table 3 captions shortened; §7 remesh-noise and capacitor-mapping paragraphs compressed. |

Skipped: Fable S2/S6 (experiments; T6 and T7 now supply both), S9 (KQCircuits/Elmer, not verified), N5 (Figure 2 legend: would change the figure), N7 (Table 3 PRIMA err), N8 (commit list); Opus N-1 (Figure 2 cap/bar overlap: figure change), N-6; Codex float-placement suggestion.

## Length

v16: 19 pages. v17: 19 pages (body to page 18, references from page 18). The
additions (Table 6, the T7 paragraph and column, the itemized rule, the grounded
bracket, the companion result, the film comparison) are paid for by cuts; reaching
18 pages would need cuts of substance.
