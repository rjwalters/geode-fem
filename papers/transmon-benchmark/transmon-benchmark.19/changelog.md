# transmon-benchmark.19 changelog

v18 → v19 is a reader-clarity revision for Crutcher Dunnavant, who is now a
coauthor: a tensor-compiler and ML-systems engineer, not a qubit or EM-FEM
specialist. The input was a pre-read "Crutcher lens" of v18, with MUST items
1–7, SHOULD items 8–17 and NIT items 18–21. Each lens claim was checked
against the v18 source and the artifacts before it was acted on.

- No solver and no `cargo` was run, and no measured result changed.
- The one new input is the velocity bands in
  `benchmarks/transmon_devicelayout_optimize/runs/fd_initial.toml`, drawn in
  Figure 1.
- All five figures were redrawn.
- The operator's rulings stand except authorship, which the 2026-10-22 ruling
  replaces.

## Lens items

| Lens item | Resolution |
|---|---|
| MUST 1 ("order" has five meanings) | **Done.** Element order $p$ ($p = 1$ linear, $p = 2$ quadratic) is defined in §3.2, together with "straight-sided" and "curved". Convergence rate $r$ (error $\propto h^r$) is defined in §5, using $r_e$ for the distance to the edge. Throughout the text, tables and figures: "order 1/2" becomes $p = 1$/$p = 2$; "observed order" becomes "observed rate"; "second order on $h$" becomes "$p = 2$ on $h_F$". The abstract now reads "Extrapolated assuming error $\propto h$, the fastest rate those edges allow on uniform meshes". The verifier bans "order 1/2", "observed order", "asymptotic order" and "second order on" in the body, and "order" in every figure. |
| MUST 2 (derivative credited, then withdrawn) | **Done.** The abstract, contribution 3 and the conclusion now say: Newton's method in DeviceLayout's variables needs 3 or 4 Palace runs with any of three Jacobians, against 10 to 12 for PRIMA; geode-fem's shape derivative gives no advantage here. Contribution 3 adds that the one-time δ calibration replaces one Palace probe run, and §7's PRIMA paragraph ends on the same statement. The Discussion paragraph now explains the claim instead of retracting it ("Three properties of this benchmark explain why…"). `check_v19()` bans "with geode-fem input", "geode-fem-calibrated" and "in 4 without" in all three places. |
| MUST 3 (title) | **Not changed (ruled title).** See "For the operator" below. |
| MUST 4 ($c$, $L$ and the "island band" not drawn) | **Done.** Figure 1(a) now shows:<br>• a $c$ dimension arrow from the island's bottom ($y_0$ = 12 µm) to its top ($y_0 + c$);<br>• the resonator outline in orange, labelled "length $L$ (total_length)";<br>• the stretched strip ($y$ 12–500 µm), shaded and labelled;<br>• an arrow for the region translated by $\Delta c$.<br>The bands come from `fd_initial.toml`. Both are asserted in `fig1_geometry.py` and checked in `verify_figures.py`. In the text, "island band" became "the strip that holds the island below its top", and the "stretched strip" wording is used in §7, Figure 1 and the caption. §3.2 defines $c$ and $L$ with the published values. |
| MUST 5 (no mesh map; $h$ overloaded; "family" undefined) | **Done.** New Table 1, "The meshes", sits in §3.2. Its rows are fixture, DeviceLayout $h$, family a, family b, adaptive, and DeviceLayout's discretization. Its columns are designs, how made, levels and used for. The fixture's levels are now $h_F$, $h_F/2$, $h_F/4$, so $h$ means DeviceLayout's mesh only. "Family" is defined in §3.2 and glossed in the intro ("two ways of subdividing the same starting mesh"). The verifier checks the rows against the artifacts: 8× tetrahedra per gmsh split; family b on initial, final and step 2; five adaptive passes. |
| MUST 6 (box models not pinned) | **Done.** Figure 1 now labels the box:<br>• (a) has a "Box wall" outline;<br>• (c) draws the box walls.<br>Text changes:<br>• §3.2 says the box walls are PEC and therefore at ground.<br>• §6 defines "natural" (no field line leaves the box) and "grounded" (walls held at ground) in plain words.<br>• The abstract has no box model.<br>• Contribution 2 quotes only the grounded bracket [79.56, 79.77] fF ("with the box walls held at ground, as in the eigenproblem").<br>• The natural/grounded distinction stays in §6, Table 6 and the Figure 2 and 4 captions. Figure 4's caption says the grounded box changes only the tensor upper bound, to 79.77 fF. |
| MUST 7 (bound, band, cap) | **Done.** One sentence in §1 defines all three: a *bound* holds by theorem and is given only for the island capacitance; a *cap* follows from a bound only under the circuit model; a *band* is an extrapolation range, which is neither. It points forward to the tiers table. The $L_s$ and $k$ ranges are now "fitted ranges" (§3.3) and "the $L_s$ and $k$ ranges" (§6, §9), never "bands". |
| SHOULD 8 (no circuit schematic; bare/dressed) | **Done.** Figure 1(d) is the two-node circuit: island node, $C_\Sigma$ across $L_s$ + $L_J$, and coupling $k$ to the resonator. §3.3 refers to it and defines *bare* (the qubit's own LC frequency) and *dressed* (after the coupling to the resonator pushes it, which is what the eigenproblem computes). |
| SHOULD 9 (intro assumes the physics) | **Done.** A new first paragraph says:<br>• a transmon is an electrical oscillator (island = capacitor, junction = inductor);<br>• its frequency is set mostly by the capacitance from a 3-D field solve;<br>• designers tune lengths to targets, so an 8 % low solve gives a design about 8 % high in the same device model.<br>The plasma-versus-$f_{01}$ caveat moved to a footnote. |
| SHOULD 10 ("consistent", then the disclosure) | **Done.** The intro sentence right after the first "consistent" says: "The extrapolation rule was chosen after that capacitance route had been computed, so their agreement is a consistency check, not a blind prediction." The conclusion says "a consistency check rather than a blind prediction". §5 now points back ("As stated in Section 1, the rule was fixed after…"), and §6's midpoint sentence cites Section 1. |
| SHOULD 11 (two denominators) | **Done.** The "sits 6.2–6.8 % and 7.6–8.4 % low" sentence (§5) and "6.2–8.4 % low" (conclusion) are deleted. Every published-vs-estimate gap is now a percentage above the published value. The artifact values are still asserted, and their absence from the text is checked. |
| SHOULD 12 (runs, solves, evaluations, steps, probes, levels) | **Done.** §7 opens by defining *Palace run* (one Palace eigensolve at DeviceLayout's discretization, about 4 min on 32 ranks), *step* and *probe*. geode-fem solves are not Palace runs and are costed in seconds. Table 4's column and Figure 5(b)'s axis and title say "Palace runs". The adaptive run has "passes", its last row is "finest pass" (Table 3, Table 7), and "third solve" became "third pass". |
| SHOULD 13 (Figure 2) | **Done.** Each panel draws the two $r = 1$ pair extrapolations, $p = 1$ ($h/2$, $h/4$) and $p = 2$ ($h$, $h/2$), as dashed legs ending in small open markers in slot a. The legend says "(estimate = mean)", and `verify_figures.py` checks that their mean is the committed estimate. A key under the legend explains the a, b and static slots. The cap line runs across the full panel. Family b's ladder points stay in Figure 3 to avoid clutter. |
| SHOULD 14 (Figure 3) | **Done.** Uniform points labelled $h$, $h/2$, $h/4$; adaptive passes numbered 1–5; dotted line at the $10^7$ size limit; "pass 5: +0.55 % (still rising)" callout; estimate panels lettered (b) and (d). |
| SHOULD 15 (Figure 4 shows only upper bounds) | **Done.** New panel (b) shows, for each adaptive round, the upper bound (hollow) and the best lower bound (filled) for the scalar and tensor models. These come from `[[adaptive_x0]]` and `[[adaptive_x0_tensor]]`, so the bracket visibly closes, and the panel title gives the gap. Panel (a) labels $h_F$, $h_F/2$, $h_F/4$. The caption gives the grounded-box difference. |
| SHOULD 16 (Figure 5(a) mixes two objectives) | **Done.** Two shaded regions carry legend entries: D for steps against DeviceLayout's discretization, E for steps against the extrapolated frequencies. A callout reads "$c$: 1218 (published) → 1446 µm (+19%)". The Newton step numbers 1/2/3 are removed, so "Step 1/Step 2" only labels the extrapolated steps. |
| SHOULD 17 ("high-fidelity" contradicts the thesis) | **Done.** "High-fidelity objective" became "the objective $F$, the frequencies at DeviceLayout's discretization", and "fidelity ratio" became "the ratio $S = F/f_h$". |
| NIT 18 (cracking) | **Done (cut).** The §4 paragraph is now three sentences. The edge census stays in Table 2's caption and the artifact. No schematic was added. |
| NIT 19 (17.49 GHz mechanism) | **Done.** The plain statement stays in the text; the free-node mechanism moved to a footnote. |
| NIT 20 (abstract density) | **Partly.** Removed: the 17–39 % band (now only in Table 6, Estimated row), the natural-box bracket and "alone". The abstract is at its 200-word cap. |
| NIT 21 (glosses) | **Done.** rhobeg ("its initial trust-region radius"), Dörfler marking (refines the elements that carry the largest 70 % of the error), "without keeping the mesh conformal", and Prager–Synge (a field that nearly balances the charges becomes a lower bound once its imbalance is paid for). The EPR was already defined. |

## Coauthor (operator ruling 2026-10-22)

| Item | Resolution |
|---|---|
| Author block | `Robb Walters \\ Independent Researcher \and Crutcher Dunnavant \\ [PENDING coauthor-affiliation]`. Plain second-author order, no `\thanks`, no equal-contribution note, no email. The v18 `% NOTE` comment is removed. The verifier pins the block and allows exactly one `[PENDING …]` marker, this one. |
| §8 → author contributions | Kept: the whiteroom paragraph and its single `dunnavant2026whiteroom` citation (palace_whiteroom URL). Added: "Author contributions: C.D. wrote that specification, gave design input and review on the solver's operator layer, and contributed code: the migration to Rust 2024, the Metal (Apple GPU) backend, the present layout of the core crate and the examples, and the geode-util crate. R.W. ran the research and the AI agents that produced the rest of the code, the experiments and the text of this paper, made every scoping and claim decision, and is responsible for the results." The code contributions are the ones v11–v18 already state under the BRIEF's v13 §8 ruling; nothing else is added. The section stays within the ruled 150–250 words. |
| Acknowledgments | Now only thanks the Palace and DeviceLayout.jl developers and points to §8. The thanks to Crutcher moved into the contributions statement. |
| BRIEF | New v19 section marked CURRENT. Front-matter `authors` updated. The 2026-10-13 solo-author ruling and the v18 coauthorship NOTE are recorded as superseded. |

## Length

v18 had 19 pages; v19 has **20**. The added material is:
- Table 1 (mesh inventory);
- Figure 1(d) and the larger Figure 1;
- Figure 4(b);
- the definitions of $p$, $r$, the three interval words and the Palace run;
- the intro's first paragraph.

These cuts paid for part of it:
- the cracking paragraph;
- the GCI sentences;
- the related-work energy-participation clause;
- the reproducibility hosts sentence (rank counts stay in the artifacts);
- "grounded minus natural in [0, 0.21] fF" in the tiers table (it stays in §6);
- the 17–39 % band in the abstract and conclusion;
- the "% low" sentence;
- the first-steps paragraph;
- the practitioner paragraph.

Bibliography leading went from 9 to 8.8 pt. Getting back to 19 pages would mean cutting Table 5 (its values appear graphically in Figure 5(c, d)) or §11's solver settings.

## For the operator

**Title (lens MUST 3).** The ruled title is "Uniform mesh refinement underestimates the frequencies of the DeviceLayout.jl transmon: an independent finite-element cross-check and re-optimization". It was left unchanged.

The lens's concern, which I checked and think is fair:
- The published frequencies did not come from uniform refinement. They came from one curved $p = 2$ discretization.
- The paper's stated main result (§1) is that the published frequencies are discretization-limited.
- A reader of the title will ask "underestimates compared to what?". The answer sits in the body: uniform levels approach from below, and the extrapolations and adaptive passes lie above them.

Proposed alternatives, in order of preference:
1. "The published DeviceLayout.jl transmon frequencies are discretization-limited: an independent finite-element cross-check, refinement study and re-optimization"
2. "Uniform mesh refinement converges slowly from below on the DeviceLayout.jl transmon: an independent finite-element cross-check and re-optimization", which keeps the ruled uniform-refinement lead and states the direction and the reason.

The verifier asserts the ruled title, so a change needs that assertion updated as well.

**Affiliation.** `[PENDING coauthor-affiliation]` is a literal marker in the author block. It must be replaced before deposit.

**Contributions wording.** "gave design input and review on the solver's operator layer" scopes his review to the operator layer, the scope v18 used for design input. If his review covered the paper itself, the sentence should say so; I did not add it, because the ruling did not state it.
