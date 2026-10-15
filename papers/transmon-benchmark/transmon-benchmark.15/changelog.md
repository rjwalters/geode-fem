# transmon-benchmark.15 changelog

Revision of v14 against two independent Anvil paper reviews of v14 held outside
the thread (no `.14.review/` sibling): Opus (37/44, advance) and Fable (37/44,
advance). The operator's rulings on #1129 stand: the title, the derivative-scope
paragraphs, a short Section 8 with no counts, and "bounded by [a, b]" with the
tiers Bounded / Model-dependent / Estimated. Two rulings are new: the
acknowledgment is approved as written (its TODO marker is removed, the text is
unchanged), and the release gate is review rounds until both reviewers score at
least 40/44. Two results from #1135 (PR #1139,
`benchmarks/transmon_devicelayout_controls/` README §6) enter the text. No
solver or `cargo` was run for v15.

## Must-fixes

| Source | Note | Resolution |
|---|---|---|
| Fable M1 (generic, major) | C_Σ has the opposite of Koch et al.'s meaning; a designer can compute 4.63 GHz from the Bounded entry | The electrostatic island capacitance is renamed **C_isl** (macro `\Cisl`) in every equation, the Figure 4 caption and y-label, the Figure 2 legend and the §6 text. **C_Σ = C_isl + C_J** follows Koch's convention and is used wherever a frequency or E_C is formed: E_C = e²/2C_Σ, the bare qubit 1/(2π√((L_J+L_s)C_Σ)), the cap 1/(2π√(L_J C_Σ)) at C_Σ ≥ 85.06 fF. The Bounded row of Table 5 now reads "Island capacitance C_isl …, without C_J: [77.40, 77.61] / [79.56, 79.76] fF; with C_J, C_Σ = C_isl + C_J in [85.06, 85.26] fF". The Model-dependent row writes the cap as 1/(2π√(L_J C_Σ)) at the lower bound on C_Σ. Nothing was recomputed: 85.06 and 85.26 are the bracket ends plus C_J = 5.5 fF. `verify_evidence.py` asserts that 4.63 GHz is what C_isl alone would give, that no √{…} in the body contains C_isl, and that no stale "C_Σ + C_J", "C_Σ ≥ 79.56" or "C_Σ of the unmodified" survives. |
| Fable M2 (generic, major) | "Order 1 is the fastest rate …" lacks "on uniform meshes" | Abstract ("the fastest rate the sheet-edge singularity allows on uniform meshes"), §5 rule paragraph and §10 Threats. A verifier loop requires "on uniform meshes" within 120 characters of every such statement. (§1 never stated the rate; the abstract and §5 did.) |
| Fable M3 (generic, major) | "Qubit frequency" is the linear (plasma) mode, not f01 | §1, at the first use in the body: "the qubit frequency is the junction's linearized (plasma) mode, not the transmon's 0–1 transition f01, which lies below it by about the charging energy E_C/h". The abstract says "All frequencies are linear eigenmodes, the junction linearized". §3.3 gives f01 ≈ √(8E_JE_C)/h − E_C/h. §10 "What the shift means for a design": the 7.0 % is a rise of the linear mode, so it is like for like with the published linear values; under the circuit model at fixed L_J, C_Σ is overstated about 14 % (1.070² = 1.145) and E_C is about 14 % larger, which does not depend on f01; f01 rises by a little less, about 6.6 %, because E_C/h rises with the mode from about 0.19 to 0.22 GHz (derived from L_J, the two frequencies and the exact SI constants; checked by the verifier). |
| Opus MF-1 / Fable S3, S4 (generic, major) | Why h/2 is the same discrete problem; Table 1 counts like for like | §4: "Cracking alone leaves the PEC problem unchanged: each volume edge attaches to one copy of a duplicated vertex, so the edges that cracking adds are copies of edges in a sheet, and the PEC condition eliminates them. Only the bisection changes the unknowns." §5 Result: Palace's h/2 logs report no bisection; its element counts equal the meshes' (1,066,840 / 1,307,448); its vertices exceed geode-fem's by exactly the logged duplicates (23,106 / 28,414); its 1,325,370 / 1,624,470 edges exceed geode-fem's 1,249,916 / 1,531,492 by the sheet-edge copies. All read from the committed logs and `geode_runs/*_h2.toml`, and the verifier recomputes them. Table 1 caption: both counts are all first-order Nédélec unknowns (edges) including PEC sheet edges, 156,863 (133,108 interior) against 183,891; "adds 4,913 elements by bisection" (Opus N-3). §1 states the h/2 claim and points to §5. |

## Should-fixes

| Source | Note | Resolution |
|---|---|---|
| Opus SF-1 | Abstract's last sentence | Rewritten as Opus proposed: "… in 3 runs, whether its Jacobian uses geode-fem's shape derivative or a scaling law calibrated by geode-fem, and in 4 with no geode-fem input; PRIMA takes 10 to 12 at three initial trust radii in one environment." The conclusion's "its" ambiguity is fixed the same way ("with geode-fem input (its shape derivative, or its calibration of the resonator model) and in 4 without"). |
| Opus SF-2 | Cap in the abstract | "… caps it at 4.477 GHz (+8.1 %)", which needs no extrapolation. The abstract now leads with the finding (Fable S10) and stays at 200 words; "+17 % to +39 % within the band" moved out of the abstract to make room. |
| Opus SF-3 | "Finest uniform level" | Replaced everywhere by "the highest uniform level(s), order 2 on h/2" (7.0 M unknowns on the initial geometry), with one clause that order 1 on h/4 has more unknowns and lower values. Fixed in §5 Result and Adaptive, §10 practitioner paragraph and Table 5; the §10 "What is established" repeat is cut. The verifier bans "finest uniform". |
| Opus SF-4 | "Which a conforming discretization can only overstate" | §6 now: "The quasi-static part of a first-order Nédélec field is the gradient of a P1 potential, and a conforming potential can only overstate the capacitance"; the 1.67 compares the P1 bound on C_isl plus C_J with C_Σ against the order-1 fixture qubit, like for like. "Heuristic, not a proof" kept. |
| Opus SF-5, SF-6 | What the limit is a limit of; family b graded | §10 Threats: every h→0 limit is of DeviceLayout's model (zero-thickness PEC sheets, lossless, linear); film thickness and kinetic inductance remove the singularity at the film scale and move both published and extrapolated values by amounts not measured here. §5: Palace's refinement first bisects where a sheet is one element wide, "so family b is graded slightly toward the sheets, a plausible reason for its higher observed orders". |
| Fable S2 | "Our Palace environment" | §5: neither the blog nor the example records the Palace version, mesher build or platform of the published runs; ours are Palace fba6a5b, gmsh 4.15.2, AWS Graviton4. Every published value lies below ours, as a slightly coarser published mesh would make it, but the 0.34–0.68 % gap is "not diagnosed". |
| Fable S1 | L1–L4, Squad | §8: "whose four layers, L1–L4, restate Palace's cited source as pure functions, fused primitives, tensor operations and a small graph-evaluation calculus" (from the palace_whiteroom repository's CLAUDE.md, read via the GitHub API); "Squad, a message room the agents share". 195 words, inside 150–250. |
| Fable S5 | Surface-participation literature | One sentence in "What a practitioner can do", citing Wang et al., APL 107, 162601 (2015), DOI 10.1063/1.4934486 ("two-step finite-element simulation", from its abstract) and Martinis, npj QI 8, 26 (2022), DOI 10.1038/s41534-022-00530-6 (analytical formulas with finite metal thickness, from its abstract). Both resolved on Crossref 2026-10-15. |
| Fable S6 | Figure 2's crowded h→0 column | `fig2_crosscheck.py`: each family has its own categorical slot ("h→0, a" and "b") on both geometries, the static route has its own slot ("static") in (a), and the DeviceLayout-discretization diamond is drawn just right of h so Palace order 2 on h shows (Opus N-4). The caption says the bands are drawn untrimmed and the cap trims them (Opus N-5). Height 4.8 → 4.5 in. |
| Fable S7 | Palace's Target semantics | Cited to Palace's configuration schema (`scripts/schema/config-schema.json` at fba6a5b: "frequency target above which to search for eigenvalues"), via `\citep{palace}`. |
| Opus SF-7, Fable D9 | Length, page 14 blank | "What is established" reduced to its pointer; the Model-dependent row gives the bracket as [4.4297, 4.477] GHz; the separate "Why the shape-derivative step lands farther" paragraph is folded into a shorter passage of §7; the §1 contribution 3 and 4 bullets are shortened; the conclusion's optimizer sentence is one clause. Result: **19 pages**, body to page 17, references on pages 18–19 (v14: 18). The new material (h/2 counts, final-design family b, PRIMA radii, f01, environment, limit, surface participation) is about one page; the cuts recovered about half of it. |
| Opus N-1 | Reviewer-facing bib notes | burn → "Software, v0.21.0"; devicelayout → "Software, v1.15.0"; palace → "Software, commit fba6a5b"; devicelayout-blog → "AWS Quantum Technologies Blog"; the devicelayout-example note is removed and its title uses a colon (Fable N6). The verifier bans the old phrases. |
| Opus N-10 | validation.md / changelog claimed Figures 2 and 4 were v13's bytes | v14's sidecars are immutable; v15's `validation.md` states the facts: Figure 1 is v9's bytes, Figures 3 and 5 are v14's, Figures 2 and 4 differ from v13 and v14. `verify_figures.py` asserts each. |

## Judge nits on v14

| Note | Resolution |
|---|---|
| "model-dependent, not bounded" | "so it is model-dependent and not in the Bounded tier"; the old phrase is banned. |
| Table 5 Model-dependent row should name the symbol | It names C_Σ explicitly in the cap formula and the quasi-static assumption. |
| SQuADDS "curated" | "what its authors call a ``validated design database''", their title's wording. The verifier's "validated" ban now exempts exactly that quoted, attributed phrase (pinned once, and checked against the bib title); "curated" is banned. |

## New results from #1135

| Item | Resolution |
|---|---|
| T4, final-design family b | §5: 3.2924 [3.2778, 3.3307] GHz at observed order 0.90 (a: 0.61), 0.83 % above family a, band inside a's; "We quote the final qubit as 3.265–3.292 GHz, +8.7 % to +9.6 % over the published 3.005 GHz." Every "one family" statement is removed (§5, abstract, §1, Table 5, conclusion); family b is higher on all three geometries (0.64 / 0.83 / 0.52 %). Table 2's footnote gives family b for all four rows (resonator final 4.113 [4.106, 4.125]). Two-family ranges: qubits +7.3 to +9.6 %, resonators +2.7 to +3.3 %, DeviceLayout's discretization 6.2–8.4 % below the estimates. All are estimates. Figure 2 plots family b on both geometries. |
| T5, PRIMA trust radius | §7 and two new Table 3 rows: 10 / 10 / 12 runs at rhobeg 0.1 / 0.2 / 0.39999999999999997; 0.4 is one ulp above (x_u − x_l)/2 in binary64 and is rejected and reset to about 0.2; PRIMA's final radius defaults to 1e-6·rhobeg (PRIMA.jl 0.2.4 `default_rhoend`; the driver passes no rhoend), so it moves too, but every run stopped on the target first (FTARGET_ACHIEVED); single runs per setting. "3 against 10 to 12" in §7, abstract, conclusion; Table 5 points to Table 3. |

## Other

- Fable N1: "central tool" → "is used at every step of the re-optimization".
- Fable N4: "its default direct solver, SuperLU" (the cross-check artifact's note).
- Opus N-2: "≤ 0.033 %" only (the "worst case 0.032 %" duplicate is gone).
- Opus N-9: "six computed modes above 4.5 GHz, one of them a port-sheet artifact".
- Opus N-7: contribution bullet rewritten without the run counts (the abstract and §7 carry them).
- Acknowledgment: `% TODO(operator): approve acknowledgment wording` removed; text unchanged (checked by the verifier). The Zenodo DOI marker remains.
- Declined: Opus N-6 (Figure 5(b) x-offset) and N-8 (unit unification; the artifacts record GB for the geode-fem RSS and GiB for the host), N-11 (split sentence), Fable N7–N10; Figure 5 is v14's bytes.

## Verifiers

- `verify_evidence.py`: v14's checks adapted, plus the guards named above (C_isl/C_Σ, uniform-mesh rate, plasma vs f01 arithmetic, h/2 Palace log counts, Table 1 wording, environment, final family b recomputed with the rule, PRIMA radii from the artifact and the BOBYQA limit, abstract and conclusion sentences, SQuADDS quote, bib notes and the two new DOIs, §8 definitions, acknowledgment marker, v14 main.tex unchanged). Eight mutations were each killed (see validation.md).
- `verify_figures.py`: the controls artifact's new hash; Figures 1, 3, 5 equal to v14's bytes and 2, 4 different; C_isl labels and no Σ in Figures 2 and 4; Figure 2 slot coordinates against the artifact for both families and all four panels; caption wording.
