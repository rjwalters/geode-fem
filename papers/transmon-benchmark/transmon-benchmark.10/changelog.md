# transmon-benchmark.9 → transmon-benchmark.10

Prepared 2026-10-13 for tracker #1063 (referenced, not closed). A restructured
revision of v9 (`main` at `264b99b8`), written to the operator's 2026-10-12 Q5
ruling: reframe around the converged DeviceLayout cross-check, then report the
re-optimization toward the published final design. It folds in #1101 (#1091,
certified sapphire-tensor brackets), #1106 (#1092 part 1, the cross-check),
#1111 (#1092 part 2, the re-optimization) and #1094 (#1093, the verified
literature re-scan, `transmon-benchmark.9.litsearch/`). It answers the two
independent v9 reviews posted on #1063 (Fable 24/44, Opus 22/44). No review,
audit or score is claimed or carried forward. v10 is a DRAFT. v1–v9, every
critic sibling and `transmon-benchmark.9.litsearch/` are byte-identical to
`main`.

**Not resolved, by design (#1063 operator items):** the title, the two "first
independent validation" sentences (verbatim; one new `TODO(operator)` comment
records the reviewers' and litsearch's recommendation to drop "first" and
"validation"), the acknowledgments, and the six inherited `TODO(operator)`
lines (verbatim; the two rendered footnote lines now sit in §9 and Appendix B
because the sections moved).

## Headline

v9: the optimizer reaches a C_Σ anchor read from the published 4.14 GHz;
certified brackets decide design against that target.

v10: the published SingleTransmon frequencies are discretization-limited.
Palace at DeviceLayout's discretization reproduces both published pairs to
< 0.7 %; geode-fem and Palace agree to 1e-4 % on the qubit on shared meshes and
converge 7.3 % / 8.7 % above the published qubits (resonators 2.7 %). A
certified tensor C_Σ bracket [79.56, 79.76] fF read through the junction
circuit agrees (4.45 GHz) and caps the initial qubit at 4.477 GHz. Newton with
geode-fem's eigenfrequency gradient meets DeviceLayout's criterion in 3 Palace
runs against PRIMA's 9; at converged resolution the 3.0/4.0 GHz design needs
c ≈ 1446 µm, +19 % (+16 % to +38 % within the band). One-sentence
contribution, §1: "the published frequencies are discretization-limited: two
independently implemented solvers converge to qubit frequencies 7–9 % above
them, a certified bound on the island capacitance gives the same shift by a
static route, and a gradient-assisted re-optimization at converged resolution
finds that the optimized design needs a capacitor about 19 % longer."

The 93.95 fF designs and their brackets are kept only as Appendix A, labelled
as aimed at a misread target. The anchor chain (93.954 → 93.853 → 93.143 fF),
its verdict table and Figure 7 are cut.

## Changes by section

| v10 section | Change | Source |
|---|---|---|
| Title, authors | Unchanged (operator item). | — |
| Abstract | Rewritten, 221 words (v9: 663), around the cross-check, the static route and the re-optimization; certified vs estimated stated in its last sentence. | #1106, #1111, #1101 |
| §1 Introduction | New framing; one-sentence contribution; five contributions (v9: seven). The "first independent third-party validation" sentence and the "doubling as the first independent validation" clause are verbatim, with a `TODO(operator)` comment. The HFSS/COMSOL "no derivative path" and "uncontested" claims are gone. | #1063 Q5, reviews, litsearch items 1–2, 7 |
| §2 Related work | Rebuilt: adjoint/direct sensitivity in FEM EM (Nikolova; Webb 2001/2002, Nair–Webb; Choi; Akçelik 2005/2008; Herter; HFSS and COMSOL documentation; UFL/NGSolve); capacitance sensitivity as classical (El-Moselhy, Ren, Park); qubit design (adds SuperGrad, Eun); complementary capacitance bounds (Pólya–Szegő, Hammond–Penman, Penman–Fraser, Specogna, Prager–Synge, Raviart–Thomas, Braess–Schöberl, Ern–Vohralík, Parés); Palace cross-checks repositioned (SQDMetal, Ye) and called code verification. "To our knowledge, within qubit EM design" wording per the litsearch. `webb2000design` not cited (byline unresolved in the litsearch); no "Nédélec" beside the Akçelik citations (not confirmed). | litsearch items 1–6, 8 |
| §3 Solvers, device, models | v9 §3–4 condensed to 1.5 pages; adds the Hellmann–Feynman eigenfrequency derivative (Eq. 1) used by the re-optimization; the sapphire rotation stated as the identity (Opus); two mesh families (fixture, DeviceLayout-regenerated) stated; the rank-1 shunt is the junction model of the ladders; circuit model with L_s and k bands labelled estimates. | #1101 meta, #1106, Opus review |
| §4 Same-mesh agreement | v9 §5–6 condensed to one section, one table (six modes plus the qubit; v9's Figure 2 duplicated it, removed). Port-sheet artifact kept as an inference. Fixture refinement shifts (+14.5 %, +20.8 %, +6.1 %) quoted next to the agreement (Opus CF2). The "withdrawn claims" narrative, gauge arc and L-scaling figure are cut. | reviews |
| §5 Cross-check (new) | Published designs, reproduction at DeviceLayout's discretization, the ladder, the extrapolation rule, Table 2, new Figure 2, same-mesh agreement on the ladder, scope. | #1106 |
| §6 Static route (new) | The certificate as a displayed derivation with the 0 ≤ φ ≤ 1 premise and N > 0 (Fable); "certified" defined at first use; scalar and tensor X⁰ brackets; certified gap 2.5–3.0 %; overstatement ≥ 71.3 % (tensor) / ≥ 77.4 % (scalar), our computation from the certified upper bounds; static route and the 4.477 GHz cap with its two caveats. Resolves the v9 critical flag on mixing scalar and tensor models. | #1101, #1106 |
| §7 Re-optimization (new) | Gradient and its check, the transmission-line column, Newton on the high-fidelity objective, Table 4 (DeviceLayout's discretization), Table 5 (converged), the converged design with its band and the shared-limit check, new Figure 4. | #1111 |
| §8 Sensitivity layer | v9 §8 condensed; issue/PR column removed from the table; stationarity attributed as classical (Park). Plate loop proof reduced to two sentences. | litsearch item 5 |
| §9 Reproducibility | Two-page path list replaced by one artifact table and a short paragraph; footnote `TODO(operator)` verbatim. | reviews |
| §10 Discussion | Evidence-tier table (certified / measured / estimated / not claimed) replaces three paragraphs; threats and limitations merged. | Fable nice-to-have 18 |
| §11 Conclusion | Rewritten to the v10 headline. | — |
| Appendix A (new) | The 93.95 fF electrostatic design loop as a certified acceptance test, with scalar and tensor brackets; its target named as a misread of the initial-geometry frequency. | #1101, #1063 Q5 |
| Appendix B | Performance context condensed (CPU table only; AMS, GPU and memory in one paragraph); the cubecl footnote `TODO(operator)` verbatim. | reviews |
| Cut | v9 §6 revision narrative, §6.2 gauge arc, §9.1–9.3 coarse tiers, §9.5 anchor chain, §10 roadmap table, §11.2–11.3 tables, v9 Figures 2–9 except Figure 1. | reviews |

Voice: "honest" 0 (v9: 17–18), "earlier versions" 0 (v9: 22), issue/PR numbers
in the body 0 (v9: 26–36), em-dashes 3 (v9: 89–107; all three are in the
inherited operator-item text). 7.4k words (v9: ~17k). **23 pages** (v9: 43),
of which 16 are body, 2 appendix and 5 references.

## Figures

| v10 | v9 | Change |
|---|---|---|
| 1 | 1 | Unchanged bytes; caption points to §6 and §4. |
| 2 (new) | — | Resolution ladders of both published designs, both modes, with the converged estimate and band, the static route and the cap (`fig2_crosscheck.py`). |
| 3 | 5 | Redrawn: scalar and tensor (P1 and P2), both certified brackets, inset with the certified gap; no design target drawn (`fig3_convergence.py`). |
| 4 (new) | — | Re-optimization: design plane, err against Palace runs with PRIMA's published end point, converged estimates and bands against the targets (`fig4_reoptimization.py`). |
| — | 2, 3, 4, 6, 7, 8, 9 | Removed (duplicates of tables, the anchor chain, superseded runs, timing). |

## Bibliography

`refs.bib` has 76 entries, all cited: the 46 v9 entries that v10 still cites, 29 entries from
`transmon-benchmark.9.litsearch/candidates.bib` (copied unchanged; the verifier
checks byte equality) and one URL entry `devicelayout-example` for the
DeviceLayout.jl SingleTransmon documentation page (the source quoted on #1063).
Process notes removed from four entries (`burn`, `devicelayout`,
`devicelayout-blog`, `palace`) and one (`tzaniovkolevpanayotsvassilevski2018parallel`);
"TVM" braced; the Halide entry has its full title (Opus).

## Verifiers

Both rewritten for v10 and mutation-tested: 45 single edits, all caught
(`validation.md`).

## Critic notes not resolved here

| Source | Note | Resolution |
|---|---|---|
| reviews | Retitle; drop "first"/"validation" | declined — operator items (#1063); `TODO(operator)` added |
| reviews | Resolve the two rendered `TODO(operator)` footnotes; repository URL and DOI | declined — inherited operator lines, kept verbatim |
| Opus | Free-DOF explanation of the 17 % unknown-count gap | declined — the Palace-log reading is not verified; counts stated without normalization |
| Opus | Stale strings in v9-era artifacts | out of scope (artifacts outside `papers/`) |
| Fable | Reconcile the two L_s readings (0.0792 vs 0.0852 nH) | moot — v10 quotes only the L_s band |
| litsearch | Read Specogna/Ren full texts for floating conductors | open — the "new" claim is worded as an application, not a method |
