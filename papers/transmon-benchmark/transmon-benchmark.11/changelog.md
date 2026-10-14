# transmon-benchmark.10 → transmon-benchmark.11

Prepared 2026-10-13 for tracker #1063 (referenced, not closed). A narrow
revision of v10 (`main` at `3fe5f706`): the operator's 2026-10-13 rulings on
#1063 (comment 6160308624), a later same-day operator ruling relayed by the
coordinator that supersedes that comment's items 3 (whiteroom) and 4
(affiliations), and the two in-scope follow-ups from the v10 Judge's approval
of PR #1119. Every number, table, figure and section of v10 that the rulings
do not touch is unchanged. No review, audit or score is claimed. v11 is a
DRAFT. v1–v10, every critic sibling and `transmon-benchmark.9.litsearch/` are
byte-identical to `main`.

## Changes

| Where | Change | Source |
|---|---|---|
| Title | "A differentiable finite-element solver for superconducting-qubit design: converged frequencies and gradient re-optimization of the DeviceLayout.jl transmon", as ruled (no punctuation or capitalization change was needed). The title `TODO(operator)` and its "REFRAME" comment are removed. | #1063 ruling 1 |
| Author block | Solo author: "Robb Walters", "Independent Researcher". Crutcher Dunnavant and the equal-contribution footnote are removed; a source comment records that he is offered coauthorship at review. The affiliations `TODO(operator)` is removed. | later ruling 1 (supersedes ruling 4) |
| §1, last contribution | "the first independent (third-party) validation of the Palace/DeviceLayout.jl stack" → "an independent same-mesh cross-check of the Palace/DeviceLayout.jl transmon stack, complementing the earlier checks of Palace by SQDMetal and Ye et al.", citing `sommers2025open` and `ye2025electromagnetic` in both places. The §1 sentence keeps "a credential, not the contribution" and now reads "… the price of entry --- but it also gives an independent same-mesh cross-check …"; "which has no publication of its own" is dropped. That `TODO(operator)` is removed. | #1063 ruling 2 |
| §9 (new) "How geode-fem Was Designed and Built" | Before Reproducibility. geode-fem implements Crutcher Dunnavant's whiteroom L1–L4 operator specification (cited) and his design input shaped the operator layer; code, experiments and drafts produced by role-separated AI agents (Claude Opus, Fable and Sonnet via Claude Code; OpenAI models via Codex), coordinated by Loom (cited) and a Squad room; every change independently agent-reviewed (numbers recomputed from artifacts, runs reproduced, pins mutation-tested); three claims this paper corrected because of reviews (the misread 4.14 GHz anchor, the scalar/tensor mixing, the static route presented as independent); counts from local git history; the human author's responsibility. Sections 9–11 of v10 become 10–12; the §1 roadmap names §9. | later ruling 3 |
| Acknowledgments | Thanks Crutcher Dunnavant for the specification and design input, and the Palace and DeviceLayout.jl developers; points to §9 for the AI-orchestrated development instead of repeating it. `TODO(operator): approve acknowledgment wording` kept. The whiteroom `TODO(operator)` is removed. | later ruling 4 |
| Bibliography | Two URL entries: `dunnavant2026whiteroom` (palace_whiteroom, https://github.com/crutcher/palace_whiteroom; `gh repo view` confirms a public repository, created 2026-05-24) and `loom` (https://github.com/rjwalters/loom), both "Accessed 2026-10-13". 78 entries, all cited. | later rulings 2, 3 |
| §10 footnote | "The geode-fem source is public at https://github.com/rjwalters/geode-fem. Archival DOI to be added at deposit." The rendered "TODO(operator)" is gone; a single source comment `% TODO(operator): Zenodo DOI` follows. | #1063 mechanical items |
| Appendix B footnote | No public tracking issue was found (`gh search issues` on tracel-ai/cubecl and tracel-ai/burn for f64, "f64 cuda", "double precision", "float64", "f64 support"; a web search; geode's own #534 names none). The footnote now states the constraint without promising a URL: "the CUDA runtime reports no f64 support, so the f64 code paths are not exercised on the GPU" (the repository's test comments record cubecl asserting `!supports_dtype(F64)`). Its `TODO(operator)` is removed. | #1063 mechanical items |
| Changelog | The §6 row of the carried v10 section table now says "certified gap 2.5–3.1 %", matching the paper (v10's own file is immutable). | v10 Judge follow-up 2 |
| `verify_evidence.py` | Operator checks rewritten against v10 (pinned by SHA-256): the new title exactly; the solo author block and its note; both validation-claim sentences, the shared phrase pinned twice, and no "first independent", "third-party", "validation of the Palace" or "has no publication of its own"; §9's required statements, the whiteroom and Loom citations and URLs; the acknowledgments verbatim, with "AI-orchestrated" exactly once in the body; the repository-URL footnote; the cubecl footnote without a promised URL; exactly the two remaining `TODO` comments and none rendered. New voice checks: §1 and the conclusion must say "is consistent with that shift" (pinned twice, located in each section), and the body may not contain "gives/give the same shift", "same shift", "by a static route", "confirm", "corroborat", "verifies that shift" or "reproduces that shift". | v10 Judge follow-up 1, rulings |
| `verify_figures.py` | Also asserts every file under `figures/` is byte-identical to v10's. | — |

Not in scope here: the v10 Judge's third follow-up (the `[0.0252, 0.0304]` "rounded outward" upper value in `benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml` and `provenance.md`) concerns an artifact outside `papers/`; the paper prints 2.5–3.1 % and is unaffected. The flaky bit-exact gradient test the Judge noted is likewise outside this PR.

**Remaining operator items:** approve the acknowledgment wording (`TODO(operator)` in source); the Zenodo DOI at deposit (`TODO(operator)` in source); Crutcher Dunnavant's response to the coauthorship offer (source `NOTE`). The ruling's model list named Opus and Fable; repository history also shows Sonnet trailers, so §9 names all three, for the operator to confirm.

24 pages (v10: 23): §9 adds about two thirds of a page.

---

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
circuit is consistent (4.45 GHz) and caps the initial qubit at 4.477 GHz. Newton with
geode-fem's eigenfrequency gradient meets DeviceLayout's criterion in 3 Palace
runs against PRIMA's 9; at converged resolution the 3.0/4.0 GHz design needs
c ≈ 1446 µm, +19 % (+16 % to +38 % within the band). One-sentence
contribution, §1: "the published frequencies are discretization-limited: two
independently implemented solvers converge to qubit frequencies 7–9 % above
them, a certified bound on the island capacitance, read through a circuit
model, is consistent with that shift and caps the initial qubit, and a
gradient-assisted re-optimization at converged resolution estimates that the
optimized design needs a capacitor about 19 % longer (an extrapolated
estimate, 16–38 %)."

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
| §6 Static route (new) | The certificate as a displayed derivation with the 0 ≤ φ ≤ 1 premise and N > 0 (Fable); "certified" defined at first use; scalar and tensor X⁰ brackets; certified gap 2.5–3.1 %; overstatement ≥ 71.3 % (tensor) / ≥ 77.4 % (scalar), our computation from the certified upper bounds; static route and the 4.477 GHz cap with its two caveats. Resolves the v9 critical flag on mixing scalar and tensor models. | #1101, #1106 |
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
`transmon-benchmark.9.litsearch/candidates.bib` (copied unchanged, except that
five entries typed `article` are retyped: `vardapetyan2008sensitivity`,
`elmoselhy2008efficient` to inproceedings, `park2019continuum`,
`raviart1977mixed` to incollection, `polya1951isoperimetric` to book, with
journal moved to booktitle or series and container fields added; the
verifier checks byte equality for the rest and field equality, with the
added fields pinned, for these five; `candidates.bib` is not edited) and one URL entry `devicelayout-example` for the
DeviceLayout.jl SingleTransmon documentation page (the source quoted on #1063),
titled as the page is ("Single-Transmon Simulation · DeviceLayout.jl").
Process notes removed from four entries (`burn`, `devicelayout`,
`devicelayout-blog`, `palace`) and one (`tzaniovkolevpanayotsvassilevski2018parallel`);
"TVM" braced; the Halide entry has its full title (Opus).

## Verifiers

Both rewritten for v10 and mutation-tested: 45 single edits, all caught, plus
28 for the Judge-review fixes, all caught (`validation.md`).

## Judge review of df35ac01 (PR #1119)

| Item | Fix |
|---|---|
| Blocking 1: the 0.22 % agreement read as independent; the rule was chosen after the static route | §5 extrapolation-rule paragraph discloses it (the earlier observed-order rule gave 4.63 GHz, above the cap; observed-order values kept only as the band's high side; the agreement is a consistency check, not a blind prediction); §6 says the same; abstract "A static route agrees" → "is consistent with this"; §1 and §11 "gives the same shift" → "is consistent with that shift"; the 0.22 % moved from §10's "What is established" paragraph to a new Consistency row of Table 8. Kept as independent: the certified cap (stated as not depending on the rule) and the finest computed levels, Palace order 2 on h/2, +4.1 % / +4.7 % over the published qubits (floored; added to the abstract, §6, §10 and the Measured row). |
| Blocking 2: certified gap rounded inward | 2.5–3.0 % → 2.5–3.1 % (outward to 0.1 %) in §1, §6, Table 8 and the Figure 3 inset. One decimal, not 2.52–3.04: the artifact's lower 0.0252 is outward against the unrounded bounds (2.521 %) but not against the printed 1.95 fF over 77.61 fF (2.513 %); 2.5 is outward against both. Figure 3 regenerated. |
| §1 qualifiers | One-sentence contribution and third bullet carry "an extrapolated estimate" and the 16–38 % band. |
| Table 8 cap | "certified on the fixture's polygonal geometry, not on the ladder meshes". |
| c mapping basis | Stated: slope at the step-1 design where step 2 started, centred on the step-2 estimate 3.005 GHz (kept, not recentred). |
| δ ±150 µm | Now "about ±175 µm (recomputed from the two committed frequencies)": 0.15 % in either f_r of `[transmission_line]` moves δ by −166/+177 µm; the verifier recomputes it. The 0.15 % noise level itself is still README prose. |
| Figure 4 | (a) "Step 1"/"Step 2" labels clear of the dashed line; the off-plot note moved beside the leg that leaves at the bottom; (c, d) ticks and the legend use Step 1/Step 2. |
| Bibliography | Five entries retyped (above); `polya1951isoperimetric` bibtex warning gone (0 warnings); `devicelayout-example` title. |
| Akçelik 2008 | "for shape optimization (2005) and for inverse shape determination of deformed cavities (2008)". |
| Verifier gaps | Pinned: the abstract's 2.4 %, the full Not-claimed row, the δ-calibration sentence, a forbidden prior-art pattern list ("first to", "for the first time", "novel", "no prior work/solver exposes …", "none exposes …", "first shape/adjoint/gradient …"; exactly one "no prior work"), plus each fix above. |

## Critic notes not resolved here

| Source | Note | Resolution |
|---|---|---|
| reviews | Retitle; drop "first"/"validation" | declined — operator items (#1063); `TODO(operator)` added |
| reviews | Resolve the two rendered `TODO(operator)` footnotes; repository URL and DOI | declined — inherited operator lines, kept verbatim |
| Opus | Free-DOF explanation of the 17 % unknown-count gap | declined — the Palace-log reading is not verified; counts stated without normalization |
| Opus | Stale strings in v9-era artifacts | out of scope (artifacts outside `papers/`) |
| Fable | Reconcile the two L_s readings (0.0792 vs 0.0852 nH) | moot — v10 quotes only the L_s band |
| litsearch | Read Specogna/Ren full texts for floating conductors | open — the "new" claim is worded as an application, not a method |
