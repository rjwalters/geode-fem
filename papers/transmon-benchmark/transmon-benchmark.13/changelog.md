# transmon-benchmark.13 changelog

v13 revises v12 against two reviews of v12 written outside the thread (no
`.12.review/` sibling exists): Opus (31/44, no critical flag) and Fable (34/44,
no critical flag). It applies the operator's rulings of 2026-10-15, recorded on
#1129 (framing and title; a short Section 8; the derivative paragraphs), and
folds in the #1129 follow-up runs (PR #1131): PRIMA re-run in our Palace
environment, a Palace-only Newton, and a second refinement family on the step-2
design. v1–v12 and every critic sibling are byte-identical.

Length: 29 → 17 pages (14.5 pages of body, 2.5 of references; bibliography 84 →
36 entries). Build: 0 overfull boxes, 0 undefined references or citations.

## Operator rulings (2026-10-15, #1129)

| # | Ruling | Resolution |
|---|---|---|
| R1 | Reframe as an independent reproduction and cross-check; title leads with the cross-check and convergence finding, no "converged frequencies"; keep the validation wording; the solver stays a central tool but no run count or design is credited to the shape derivative | Title: "Uniform mesh refinement underestimates the frequencies of the DeviceLayout.jl transmon: an independent finite-element cross-check and re-optimization" (the working title, unchanged). Abstract and §1 lead with the reproduction, the same-mesh agreement and the refinement finding. Contributions reordered: refinement study, certified brackets, re-optimized design with controls, same-mesh agreement (v12's contributions 4 and 5 removed/folded). The ruled validation sentence appears once, in §1. §7 retitled "Re-Optimization and Its Controls"; the shape derivative is described as a tool that supplies one Jacobian column. Verifier pins the title (no "converged frequencies", "differentiable" or "gradient") and rejects crediting phrases. |
| R2 | One or two paragraphs on why the derivative gives no advantage here, and where derivatives pay off, pointing neutrally to the conformal-antenna companion | §10 "Why the shape derivative gives no advantage here" (two paragraphs): two nearly decoupled parameters (§7 now quantifies it: a probe moves its own mode 4.5 %, the other ≤ 0.05 %), so Newton converges with any reasonable Jacobian including the free scaling law; an FD gradient costs N+1 = 3 solves; the coarse-mesh fixed-topology derivative is the slope of a different discrete problem, so a remeshed FD of the evaluated objective is the more faithful slope (here 1.5 % apart). Then: adjoint gives all N derivatives for about one extra solve; coupled targets without a scaling law; freeform shapes. Companion `walters2026conformal` (title from `conformal-antenna-diffopt.5`) cited as "its comparison with gradient-free search is still being run, so it does not yet show whether the derivative helps there". |
| R3 | §9 short (150–250 words), keep Crutcher's credit with the whiteroom citation and the AI-model statement; drop counts, provenance and corrected-claim examples; drop `count_provenance.py` if unused | §8 (renumbered: the sensitivity-layer section is gone) is 166 words: the whiteroom spec, design input and named code contributions (no commit count), the Claude (Opus, Fable, Sonnet) and Codex (GPT Sol, GPT Astra) statement, Loom/Squad, separate-session review, human responsibility. Counts, the three corrected-claim examples and the "earlier full draft" sentence removed. `count_provenance.py` and `check_provenance` removed; the verifier checks the word count and rejects counts. |

## New results (#1129, `benchmarks/transmon_devicelayout_controls/` README §5)

| Result | Where |
|---|---|
| PRIMA in our environment: 10 runs; run 9 err 1.15e-4 misses by 15 %; runs 1–5 BOBYQA's 2n+1 initial set; the published final already meets the criterion in our environment (1.5e-5) | §7 "Against DeviceLayout's discretization", Table 3 (new row), Figure 5(a, b) (trajectory and end point), abstract, §1, tiers, conclusion. The comparison is now 3 Newton runs against 10 in one environment, with the published 9 as context. New citation `powell2009bobyqa` (report read). |
| Palace-only Newton (δ = 563.879 µm): 3 runs, no geode-fem input | §7, Table 3 (new row), Figure 5(b), §1 bullet 3 |
| Second refinement family at step 2: 3.0203 [3.0033, 3.1014], resonator 4.0225; capacitor +19.1 % / +20.4 %; scaling-law union +17.1 % to +36.9 % | §7 "The re-optimized design", Table 4 (new row "Step 2, family b"), Figure 5(c, d), tiers. The "+19 % rests on one family" complaint no longer applies: the capacitor is quoted as 19–20 % over two families. |

## v12 review must-fixes

| # | Source | Note | Resolution |
|---|---|---|---|
| 1 | Fable M2/M3 (major) | Adaptive agreement and the 10^-4 % reproduction are qubit-only | Abstract: "10^-4 % on the qubit and 0.03 % on the resonator"; §1: "to 10^-4 % on the qubit"; §5 states the resonator separately, and "the adaptive run checks the qubit estimate only"; the abstract no longer claims the adaptive run falls inside the band; conclusion "reproduces Palace's qubit". |
| 2 | Opus MF-2b | "4.7 times closer" for a quadratic err | "an err 4.7 times lower …, about 2.2 times closer in frequency" (√4.67 = 2.16, recomputed). |
| 3 | Opus MF-3, Fable S11 | The cited theory allows a rate slower than O(h); order-1 estimates lean low | §5: "at best O(h), and slower"; the rule paragraph says order 1 is the fastest rate the theory allows, so the rule makes the smallest correction and its estimates lean low, and that the order-2 pair's order is theory, not observed; the common-limit fits (0.5–1.3 % above) are cited as consistent with that; abstract and conclusion say the estimates lean low. |
| 4 | Opus MF-4, Fable m4 | Recentre the capacitor band on 3.0 GHz (about +17 % to +39 %) and reconcile with the two-family union | Linear map now centred on the 3.0 GHz target: [1423, 1690] µm, about +17 % to +39 % (recomputed: +16.85 to +38.80 %); under the f ∝ c^-1/2 scaling the same band (the two-family union, since family b's band lies inside a's) gives +17.1 % to +36.9 %, which contains family b's +19.0 % to +26.9 %. Abstract, §7, tiers and Figure 5(a) updated; the verifier asserts the plotted band's x data. |
| 5 | Fable M5 | Explain why the shape-derivative first step was worse | New §7 paragraph from the Jacobian entries at the initial design: the scaled derivative gives ∂F_q/∂x1 = 3.66 GHz, within 1.5 % of Broyden's Palace probe (3.71), so it measures the local slope well; both local slopes are 11–12 % shallower than the scaling law's F_q/x1 = 4.168, and the first step is long, so a local tangent overshoots (c = 1338.338 and 1320.522 µm against about 1230 at the criterion), while the scaling law lands at 1196.933. This replaces Fable's suggested remesh explanation, which the numbers do not support (the remeshed order-1 FD is steeper than all three). |
| 6 | Opus SF-5, arXiv overlay | Nelson cited for the eigenvalue formula | Eq. (1) now cites Fox and Kapoor (1968), "Rates of change of eigenvalues and eigenvectors", AIAA J. 6(12), DOI 10.2514/3.5008 (Crossref-resolved), which gives the mass-normalized symmetric eigenvalue derivative. Nelson is cited only for eigenvector derivatives (§10). The Jacobi–Davidson pointer (`liang2026adaptive`, unchecked) is dropped. |
| 7 | Opus SF-6 | Delete "to our knowledge, no prior work" | Deleted; the verifier rejects "to our knowledge" and "no prior work". |
| 8 | Opus SF-4 | The adaptive 4.4297 GHz (+7.0 %, no extrapolation) in the abstract | Abstract, §1, §10 and conclusion. |
| 9 | Opus SF-7, Fable S15 | Fig. 3(b) plots the unreliable resonator fit | Removed from Figure 3(b); the caption says why; the verifier asserts it is not drawn. |
| 10 | Fable S10, S16 | Practitioner remedy: graded or hp meshes toward the sheet edges | §10 "What a practitioner can do", citing Demkowicz (2006), with the adaptive run as the measured instance. |
| 11 | Opus SF-1, Fable S1–S9 | Length 29 pp → about 16–17 | 17 pp. Dropped: Appendix A (electrostatic design loop), Appendix B and its table (performance), §8 sensitivity layer and its table, the shared-limit table (one sentence remains), the capacitance table (extremes quoted in §6, all levels in Figure 4), the artifact table (captions name artifacts; a paragraph gives the rest), the §9 counts and examples. Merged v12 Tables 5 and 6 into one controls table. §2 roughly halved (tensor-stack paragraph and the HFSS/COMSOL/Webb/Firedrake list cut). Abstract 290 → 194 words. Repeated "estimate, not a bound" removed (tiers defined once in Table 5; the verifier allows at most two). Bibliography trimmed to the 36 cited entries; `url` dropped wherever a DOI is given; proper nouns brace-protected (Opus N-5). |
| 12 | v12 Judge nit | Intro bullet "no geode-fem solve" | Now "the latter also with no geode-fem input at all", which the Palace-only Newton makes true; the verifier rejects "no geode-fem solve". |
| 13 | v12 Judge nit | Palace AMR "an independent check" → "a further check" | "A further check comes from Palace's adaptive mesh refinement". |
| 14 | v12 Judge nit | §5 "The converged values are estimates" is forbidden wording | Paragraph removed; "converged" no longer names an extrapolation anywhere (the verifier rejects "converged frequenc/value/estimate/design/resolution/…" and "the converged"). Figure 2's "conv." column is now "h→0", and figure legends say "extrapolated estimate". |
| 15 | v12 Judge nit | Note the frozen-verifier hash breakage | PR #1131 regenerated `benchmarks/transmon_devicelayout_controls/results.toml` (SHA-256 `8649181a…` → `f300b4ca…`), so the frozen v12 `verify_figures.py` input-hash check no longer matches current main (as v10 and v11 broke with the `c_sigma_bracket_tensor.toml` relabel). v12's figures and text are unchanged; v13's manifest carries the new hash. |
| 16 | PR #1131 Judge nits | README §5 T1 "build" → "environment"; note that famcv2_u1_p2 ran 7200 s and exited cleanly | Separate benchmarks commit (see below). |

## Other review notes

| Source | Note | Resolution |
|---|---|---|
| Opus MF-5 | Cross-build PRIMA comparison | Resolved by the #1129 re-run (row above). |
| Opus SF-3, Fable M4 | Palace-only Newton | Resolved by the #1129 run (row above). |
| Opus SF-2 | Headline design on one family | Resolved by the #1129 family-b ladder at step 2. |
| Opus SF-8 | Say what "independent" covers | §3.1: the codes share the formulation, the port, the materials and the input mesh, and geode-fem follows a specification of Palace's operators; assembly, eigensolver and linear algebra are separate. |
| Opus N-1, N-2 | "converged" 31×; "committed" 17× | Neither word names an extrapolation or a repository state in the body ("archived" is used twice). |
| Opus N-4 | Pin the date | `\date{October 2026}`. |
| Opus N-7 | "trims it to" | "trims the band to". |
| Opus N-9 | PRIMA "n/a" | Published row says "unpublished"; our-environment row "n/a" with the caption explaining the initial model. |
| Opus N-10, Fable nit | Abstract "finest computed levels" vs the adaptive level | The abstract gives the adaptive level; §5 says "finest uniform levels". |
| Opus SF-7, Fable S15 | Figure 4 bars invisible, colour mismatch, X^0 label | One hue per model (blue scalar, orange tensor), one marker per order; bars carry end marks; y label "unmodified device". |
| Opus SF-7 | Figure 5(b) y-axis to 10^4 | Capped at 1. |
| Fable S14 | GB vs GiB | Adaptive peak "93 GiB". |
| Fable S13 | Mesh/family naming | §3.2 names the fixture as an earlier generator's mesh of the initial geometry; "family" is used only for refinement schemes. |
| Fable nit | "Open tools now cover the whole loop" | Removed. |
| Fable nit | "the methods are not new" twice | Once ("The method is not new"). |
| Fable nit | cubecl clause | Removed. |
| Fable nit | 17.49 GHz "an explanation that is an inference" | Rewritten in two sentences: "This mechanism is our inference, supported by a synthetic test". |
| Fable S16 | "no methods publication" should carry the date | "As of October 2026, neither Palace nor DeviceLayout.jl has a methods publication". |
| Fable S12 | Context for the 7 % against fabrication spreads | Declined for length; the limitations paragraph states the model is lossless and linear and not a fabricated device. |
| Opus SF-9 | Venue fit | No change; advisory. |
| Fable nit | `ragankelley2013halide`, `hoffmann2009comparison`, `polya1951isoperimetric` entry types | Moot: those entries are no longer cited. |

## Separate benchmarks commit

`benchmarks/transmon_devicelayout_controls/README.md` §5: T1's heading and text say
"environment" where the comparison is between Palace set-ups (the mesh generator
also differs), and T3 notes that `famcv2_u1_p2` ran for 7200 s and exited
cleanly, with no time limit cutting it short. `results.toml` is unchanged.

Pending (carried): Zenodo DOI (`TODO(operator)`), acknowledgment wording (`TODO(operator)`), Crutcher Dunnavant's answer to the coauthorship offer (source NOTE).
