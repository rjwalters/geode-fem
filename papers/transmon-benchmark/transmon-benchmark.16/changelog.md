# transmon-benchmark.16 changelog

Revision of v15 against two independent Anvil paper reviews of v15 held outside
the thread (no `.15.review/` sibling): Opus (37/44, advance) and Fable (39/44,
advance). The operator's rulings on #1129 stand: the title, the derivative-scope
paragraphs, a short Section 8 with no counts, "bounded by [a, b]" with the
tiers Bounded / Model-dependent / Estimated, the approved acknowledgment, and the
release gate (both reviewers at least 40/44). No solver or `cargo` was run for
v16, and no artifact changed. Text, bibliography and figure-script changes only.

## Step 0 (Fable M1): the box condition, checked against the code first

`crates/geode-core/src/assembly/electrostatic_dual.rs` states that "the outer
boundary is natural (zero normal flux) except where a conductor lies on it", and
its face sum runs over "boundary faces (one side)". `certify_impl` adds
`boundary_flux_l1` (non-conductor faces with one adjacent tetrahedron) to the
penalty with the interior jumps and the divergence. The bracket example's
conductors are the three metal sheets (island driven, feedline floating, ground);
the `exterior_boundary` group is not a conductor. Fable's reading is therefore
correct. The monotonicity argument holds: $C_{\rm isl}$ is a minimum of the
energy over admissible potentials, and grounding the box shrinks that set, so
$C^{\rm PEC\,box}_{\rm isl} \ge C^{\rm natural}_{\rm isl} \ge 79.56$ fF. The lower
bound, and with it the 4.477 GHz cap, transfer to the eigenproblem's box. The
Bounded tier is not at stake.

## Must-fixes

| Source | Note | Resolution |
|---|---|---|
| Fable M1 | Box condition, boundary term, transfer to the PEC box | §6 "The bound": "and zero normal flux on the vacuum box"; Eq. (3) gains $-\lVert D\cdot n\rVert_{L^1(\mathcal B)}$, defined as the one-sided flux on the box faces "which the zero-flux condition requires of the exact field but not of $D$". New paragraph "Two box models": the static problem leaves the box natural and the eigenproblem grounds it; grounding can only raise $C_{\rm isl}$, so the lower bound and the cap hold for the grounded box; the upper bound and the dressed range belong to the natural box; the size of the difference was not computed. "From capacitance to the qubit frequency" adds the model difference to why the 0.22 % is a consistency check. The verifier asserts the source strings of the module docs, of `certify_impl` and of the example. |
| Opus MF-1 | High end of the band for the order-2 pair | §5 rule: the third pair of values is "the same pairs at the observed order of the order-1 ladder's three levels (0.62 and 0.61 for the family-a qubits), borrowed by the order-2 pair on the assumption that both element orders share the pre-asymptotic rate". It then names the ends: the order-1 ladder sets both (λ value low, three-level value high), except that the order-2 λ value sets the low end on family-b resonators; initial qubit 4.415 and 4.677 GHz, with the borrowed-order order-2 value, 4.582 GHz, inside. The verifier recomputes the six values on all 16 ladders with a band and checks every one. |
| Opus MF-2 | Figure 2's estimate labels sit on the order-2 series | The label ("a: 4.444 GHz (+7.3% vs published)") moves into the "h→0, a" slot column, low in each panel, where no ladder level, band or reference line reaches. Checked visually at 130 and 300 dpi. `verify_figures.py` asserts that each label is centred on its slot and that its box touches no line, marker or band. **Doctor correction:** the first version of the overlap test skipped every line whose transform contained `transAxes`, which also skipped every data series, so only the published line was tested; it now transforms each line with its own transform and tests all of them. With that fix, v15's placement fails on the overlap with "Palace, order 2" even with the slot-centre test disabled. |
| Opus MF-3 | Richardson and GCI | §5: "The rule is Richardson extrapolation \citep{richardson1911approximate} at the asymptotic order rather than the observed one. Roache's grid convergence index \citep{roache1994perspective} scales the correction at the observed order by a safety factor; with the observed order below the asymptotic one, it would rest on the band's high end, which for the initial qubit the circuit-model cap rules out." Both DOIs resolved on Crossref. The dispatch's "would centre on the band's upper end" is worded as "would rest on": the GCI interval is centred on the finest-grid value, and its error estimate is the observed-order correction, whose extrapolated value is the band's high end. |

## Should-fixes

| Source | Note | Resolution |
|---|---|---|
| Opus SF-2, Fable S1 | E_C from the bound | §10: $E_C/h = e^2/2hC_\Sigma$ in [0.2271, 0.2278] GHz (rounded outward from both the unrounded and the rounded brackets), Bounded as a function of $C_\Sigma$ and model-dependent as the transmon's charging energy. **Doctor correction (each number now names its model):** under the two-node circuit with the $L_s$ and $k$ bands, the published 4.14 GHz implies $C_\Sigma$ in [98.59, 98.72] fF (the committed `[anchor]` `c_sigma_band_*` of `transmon_loop_inductance`, plus $C_J$), an overstatement of 15.6–16.1 %. "At least 16.6 %" holds only for the bare reading ($L_s = k = 0$: 99.45 fF). In the bare reading the static-route range implies 85.8–86.1 fF (15.5–15.9 %) and the adaptive level 86.87 fF (about 14 %); about half of the 14 → 16.6 % gap is the still-rising adaptive level and the rest is $L_s$ and $k$. Table 5's Bounded row carries $e^2/2hC_\Sigma$; the Model-dependent row its reading as $E_C/h$. |
| Opus SF-3, Fable S2 | Family b in Table 2 | Table 2 now has "Family a" and "Family b" estimate [band] columns; the footnote keeps only the cap trim. The verifier recomputes each family-b estimate from its levels and checks band containment. |
| Fable S3 | Band beside 19–20 % | Abstract: "19–20 % longer than published (17–39 % within the band)"; conclusion: "or 17–39 % within the band". |
| Opus SF-1, Fable S4 | §7 paragraph and the binary64 aside | Split into "Against DeviceLayout's discretization", "PRIMA in our environment" and "Where the first steps land". The 0.39999999999999997 value and the final-radius note are a footnote; the body says "at the largest radius BOBYQA accepts on DeviceLayout's box, $(x_u - x_l)/2$". The landing paragraph is cut to its slopes (the 0.07 % probe detail and the scaling law's landing point leave the text). The N-11 sentence is split. |
| Opus SF-4, Fable S5 | Cracking census and counts in prose | §4 keeps "almost all on the resonator and feedline sheets"; the 1,007 edges (993 on the resonator's ground plane and the feedline) are in Table 1's caption. §5 keeps the claim and drops the six h/2 counts, which stay in the logs and are still recomputed by the verifier. The second-family paragraph no longer repeats Table 2's numbers. The practitioner paragraph points back to §5 instead of repeating the 2.2/7.0 million comparison. |
| Opus SF-5 | One mapping for the capacitor range | Both families both ways: linear (+17 to +39 %, +19 to +28 %) and scaling law (+17 to +37 %, +19 to +27 %); "Under either mapping the first range contains the second." (Opus's "+19 % to +37 %" for family a under the scaling law is +17.1 % to +36.9 % in the artifact's `union_c_band_over_published_pct`.) |
| Opus SF-6 | Adaptive practice and Ye et al.'s 0.3 % | §2: Ye et al. match resonator frequencies to within 0.3 % "with a mesh refined at the CPW gaps and fourth-order elements, a setting their convergence study puts within ±0.3 % of extrapolated values"; energy-participation analyses check convergence over adaptive passes (Minev et al. 2021, Methods). Both read from the papers' text. |
| Opus SF-7 | Search scope of the absence claim | "A web search in October 2026 of the complementary-bound and qubit-design literature (queries such as ``guaranteed capacitance bounds qubit'') found no ...", from the committed litsearch log (`.9.litsearch/notes.md`, Search log item 6). |
| Fable S7 | Quasi-lumped route | §2: the two-node circuit is a minimal instance of the quasi-lumped route (Minev, McConkey et al. 2021, arXiv:2103.10344, verified on arXiv and DataCite), against the energy-participation route. The paragraph heading becomes "Two-sided capacitance bounds and circuit models". |
| Fable S6 | New adaptive run | Not run (new experiment; optional). |
| Queued (Judge) | §8 L2 wording | "restate Palace's cited source as pure functions, unfold those into compositions of base algebraic primitives, recast them where possible as global tensor operations, and express them in a small graph-evaluation calculus", from the whiteroom repository's `CLAUDE.md` (L1 mutation rotation, L2 fusion rotation, L3 iteration rotation, L4 calculus). The verifier's pinned text is updated with it and "fused primitives" is banned. §8 stays within 150–250 words. |
| Queued (Judge) | Table 5 placement, half-blank page | No page is now under-filled (the largest internal gap is about three lines); see validation.md. |
| Queued (Judge) | 0.62 against 0.61 | Checked against the ladder levels before editing. The initial qubit's observed order is 0.615008, which rounds to **0.62**; the final qubit's is 0.612786 (0.61). v15's single "0.61" came from double rounding the artifact's 4-decimal 0.6150. The controls README §3 ("against a's 0.62") is therefore correct and is **not** changed; the paper now prints 0.61–0.62 / "0.62 and 0.61", and the verifier computes the orders from the levels. |
| Queued (Judge), Opus N-1 | 4.1 % → 4.2 % | 4.311844 / 4.14 = +4.151 % prints as 4.2 % (§5 and Table 5); the verifier rounds instead of flooring. |
| Queued (Judge) | Family-b point in Figure 5(c, d) | Not done (optional). Figure 5(c, d) already carries step 2's family b; Figures 3 and 5 are v15's bytes. |

## Nits

| Source | Resolution |
|---|---|
| Opus N-2 | Abstract: "to $2\times10^{-4}$ %" on the qubit, which covers the re-optimization ladders. |
| Opus N-3, Fable N1 | "0.0000 %" replaced by "to all six archived decimals" (§1) and "not at all in the six archived decimals" (§5); the two h/2 resonator values are equal to six decimals on both geometries. |
| Opus N-5 | Run 9's err printed 1.150e-4; the published pair's 1.151e-5 (Figure 5 caption). |
| Opus N-7 | Reproducibility moves after the conclusion, before the acknowledgments. |
| Opus N-9 | First contribution: "which puts its qubits 7.3–9.6 % above their published values". |
| Fable N4 | "for a series that rises under refinement" in the rule paragraph. |
| Fable N5 | Figure 3 caption: drawing note dropped; "unreliable and not drawn". |
| Fable §6 comment | "The range is an estimate, because $L_s$, $k$ and the resonator estimate are." |
| Contribution 2 | "the lower bound caps the initial qubit" (only the lower bound enters the cap). |

Not changed: Opus N-4 (Table 4 err column), N-6 (the sentence it names was cut), N-8; Fable N2, N3, N6–N9.

## Length

v15: 19 pages, references from page 17. v16 adds the box-model paragraph, the
Richardson/GCI sentences, the band-end definition, the E_C statement and three
related-work sentences, and is also 19 pages (references from page 18). The
references are set at 8/9 pt instead of 8/9.5 pt, and Figures 1 and 2 are
shorter (no font change). Reaching 18 pages would need cuts of substance; none
was made.
