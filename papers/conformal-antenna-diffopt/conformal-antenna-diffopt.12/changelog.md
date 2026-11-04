# Changelog: conformal-antenna-diffopt.11 → .12

Input: a reading-flow review of v11 for an interested non-specialist reader (ordering, concept
introduction, consistency, flow friction), 2026-10-23. v12 reorders and glosses; it changes no
number, result, claim scope, title, author or DOI, adds no reference, and keeps the paper's voice
("bounded", never "certified"). Specifically:
- every number in v12 appears in v11 and every number in v11 appears in v12 (`check_v12` pins the
  set equality and the exact multiset delta, which is the cost table's restated numbers plus the
  trimmed Conclusion);
- the abstract, title, author block, date and DOI are byte-identical to v11's;
- `refs.bib`, `figures/` and `main.bbl` are byte-identical to v11's; every v11 table body is
  carried over verbatim (two column headings read "Unknowns" instead of "DOFs", Table 1 gained
  one row and four glosses);
- no solver, mesher, `cargo` or other computation was run;
- v1–v11 and their critic directories are unchanged (`verify_evidence.py` diffs them against the
  merge base with `origin/main`).

## New body order

| v11 | v12 |
|---|---|
| 1 Introduction (with Table 1 on p3; Figures 1 and 2 on p5 and p7) | 1 Introduction, with the primer block Table 1 (p2), Figure 1 (p3), Figure 2 (p4) and "Position of this work" as its last paragraph |
| 2 Related Work | 2 Method, opening with 2.1 The fixture (the antenna and the coarse mesh, from v11 §4) |
| 3 Method | 3 The Adjoint Against Gradient-Free Search (+ the cost table) |
| 4 Fixture and Forward-Model Accuracy | 4 Forward-Model Accuracy |
| 5 The Adjoint Against Gradient-Free Search | 5 Design Procedure |
| 6 Design Procedure | 6 Results (+ "Air margin and curvature" from v11 §4) |
| 7 Results | 7 Related Work |
| 8 Limitations and Future Work | 8 Limitations and Future Work |
| 9 How geode-fem Was Designed and Built | 9 Conclusion, then Acknowledgments |
| 10 Reproducibility | 10 How geode-fem Was Designed and Built |
| 11 Conclusion | 11 Reproducibility |

Table and figure numbers shift accordingly: v11 Tables 2–7 are v12 Tables 4, 2, 5, 6, 7, 8 (Table 3,
the costs, is new); v11 Figures 3 and 4 are v12 Figures 4 and 3 (the figure files keep their v11
names, `fig3_accuracy` and `fig4_optimizers`).

## Proposals, applied or skipped

| # | Proposal (effort) | Resolution |
|---|---|---|
| 1 | Body follows the Intro's three-result order (M) | Applied, the reviewer's option (a): "Antenna" and "The coarse mesh" become §2.1 The fixture (`sec:fixture`); the Baselines section precedes Forward-Model Accuracy, so the order is Method, Baselines, Accuracy, Procedure, Results. The Intro's coarse-model sentence and Table 1 point at `sec:fixture`; the artifact table's row names `sec:fixture`, `sec:accuracy` and `sec:results`. |
| 2 | Primers next to the Names table (S) | Applied. Table 1, Figure 1 and Figure 2 are encountered together after the cost-measure paragraph and land on pages 2, 3 and 4 (Figure 1 is a full-page float, so Figure 2 cannot share page 3 with it). Both figure captions are unchanged. |
| 3 | Related Work after Results (S) | Applied: §7, between Results and Limitations. "Position of this work" closes the Introduction, glosses "open-radiator" and points at §7. The Related Work pointer "the curvature here is mild" now cites `sec:results`, where the curvature paragraph lives. |
| 4 | Say in the Intro why there is a coarse model (S) | Applied, one sentence after "optimizes the band objective by L-BFGS": the $p = 1$-only adjoint explains optimizing at $p = 1$; the coarse mesh L1 is where each solve is cheap (a rim-refined optimization is untried, Limitations). The formal statement at the end of §2.5 stays. |
| 5 | Designs A and C not discussed before they exist (M) | Applied. The air-margin and curvature sentences move to Results as "Air margin and curvature", placed before Table 7 and "The lower-edge budget" (which now says "above" instead of citing §4); the UPML-ladder sentence stays in §4 as "Absorber". "Port-free resonance" says "design~C of Section 5" and "for design~C"; the Palace paragraph says "design~A (Section 5)". |
| 6 | Split "Setup"; retire "several settings" (S) | Applied: "Setup", "Settings" (opens "Each gradient-free optimizer was run at two or three values of its main setting") and "Counting solves" (opens the reconciliation with "The adjoint run's recorded 190 driven solves ... include 65 that are not counted, 35 for ... and 30 for ..., leaving the 125 of Table 2"). The wall-time sentence moved to "The forward solver and the wall-clock". |
| 7 | Split "Scaling" (S) | Applied: the break is at "The adjoint's final level is set by step rejection", headed "Why some gradient-free runs end lower" (the review's "lower" meant the gradient-free runs' deeper levels, so the heading names them). The σ₀ = 1.5 caveat closes the scaling paragraph; the four-parameter sentence moves to "Where the descent stopped", where "the rectangle" needed it as antecedent. |
| 8 | Conclusion before the back matter (S) | Applied: Limitations, Conclusion, Acknowledgments, How geode-fem Was Designed and Built, Reproducibility. The last two stay numbered sections (the class is `article`; `\appendix` would relabel the cross-references for no gain). |
| 9 | Define fixture, open-radiator, feed band, cold, σ₀ (S) | Applied: a "Fixture" row in Table 1; "open-radiator" glossed in "Position of this work"; "feed band (the strip of the $+y$ edge around the probe)"; "from c = 0 (a cold start, −1.47 dB)"; "start from an initial step size σ₀" in Setup, and Settings names σ₀ as CMA-ES's setting. |
| 10 | One noun for edges / unknowns / DOFs; driven solves (S) | Applied: "has 150,561 edges, hence 150,561 unknowns at p = 1"; Tables 4 and 6 head their count column "Unknowns"; Table 1's "EM solve" row defines a driven solve; "Counting solves" glosses the recorded count as the one Table 5 uses. |
| 11 | Point counts for the two "worst" quantities (S) | Applied: Table 1's "Worst-of-band" row states the convention (five objective points, Table 2; nine verification points, Table 6); Table 2's caption says "over the five band points"; "On the finest models" says "over the nine points of Table 6". Where the paper does not state the point set (design A at P2/L1, the flat/bent comparison) no count was added. |
| 12 | Signpost the re-fitted shift (S) | Applied, after the chain is introduced: "The shift is re-fitted at each descent as the design moves: the cold optimum's 47.85 MHz had become 44.94 MHz by the last descents (Table 5)." |
| 13 | Align the drift statements (S) | Applied: "2.366–2.380 GHz: the floor is the finest computed value, and the two extrapolations put the limit 6.4 MHz (Richardson) to 13.5 MHz (rim fit) above it, the range that Table 7 carries." The "0 to 14 MHz" phrasing is gone (both numbers remain elsewhere). |
| 14 | One cost table (M) | Applied as Table 3 in §3, after "The forward solver and the wall-clock", built only from numbers in v11: the adjoint and the fastest gradient-free run to −9.5 dB (35 and 385 counted solves, estimated 666 s and 5,552 s at the r6i.4xlarge rate), the Palace-driven CMA-ES seeds (385–805 solves, measured 84–172 min) and NEWUOA (420, 90 min), the finite-difference control (2,535 solves, 4.7 h on the 16-vCPU host, 1,164–1,211 s per gradient), the final descent to C (125 recorded solves, 26 min, 32-vCPU host), the two Palace sweeps of C (3.5 h and 1.5 h on 16 MPI ranks; both logs read "Running with 16 MPI processes", now asserted) and the combined-mesh solve (one frequency, 204 GiB, about 8 min). The Results "Cost" paragraph is removed. Hosts are named only as the paper or the records name them. |
| 15 i | Trim the Conclusion's near-copy of the abstract (S) | Applied: the Conclusion states the measurement once in the abstract's "needs 385 (11×)" form, adds what follows from it (the adjoint buys solves, not a better optimum, and buys them on the coarse model), states the coverage claim by reference to Results and the next steps by reference to Limitations. It no longer repeats "1.6 mm probe", "4.3 MHz" or the "11 times fewer" form. |
| 15 ii | Relocate the air-on-cylinders note (S) | Applied: last sentences of "The morph" in §2.4. |
| 15 iii | The probe-width sentence (S) | Applied: "The probe keeps a realizable width (1.6 mm, modelled as a lumped port; an intermediate design had widened it to 4.27 mm, Section 5)." |
| 15 iv | The set-aside 53.6 MHz run (S) | Applied: a footnote; the sentence on the independent geode-fem check stays in the text and refers to "the choice between the two runs". |
| LP | "bounded" tier claim-then-deny | Applied: "We reserve *bounded* for a proven two-sided bound and use it for no result in this paper; every result here is either *model-dependent* ... or *estimated* ...". The "(not a bound in the sense of Section 1)" remark in Results stays. |
| LP | λ undefined; λ also the population | Applied: "with λ the solution of that adjoint system" (no new formula); the CMA-ES population is written "a population of 4 + ⌊3 ln n⌋ samples", so λ now has one meaning. |
| LP | UPML never expanded | Applied in Table 1. |
| LP | "dual arithmetic" gloss | Applied: "(forward-mode automatic differentiation)". |
| LP | 9.0 MHz relative to what | Applied: "a further 9.0 MHz beyond the 0.2 mm-rim level". |
| LP | "rectangle" for two objects | Applied: "a design in the four-parameter basis of Section 2.4" and "its gradient norm grew (0.023 to 0.074) where C's fell"; "rectangle" now only names design A's bounding outline. |
| LP | "11 times fewer" | Applied (see 15 i). |
| LP | Related Work's aerodynamic precedent before the measurement | Resolved by proposal 3. |
| (verdict) | The FDTD paragraph "would split cleanly" | Applied cheaply: a paragraph break before "The flat board dips", no new heading. |

Skipped: nothing in the review was skipped outright. Two sub-options were not taken: proposal 1's
option (b) (reorder the Intro bullets instead of the body), superseded by (a); and proposal 8's
"unnumbered or appendix" labelling of the last two sections, for the reason in the table.

## Review pass (PR 1244)

- The Intro sentence no longer makes the $p = 1$-only adjoint the reason for optimizing on L1: "implemented at first element order only, so optimization runs at $p = 1$, on a coarse mesh (L1) where each solve is cheap"; re-pinned in `check_v12`.
- "Counting solves" puts the 35 + 30 breakdown next to the 65 it decomposes, then "leaving the 125 of Table 2".
- "Settings" says "Each gradient-free optimizer".
- Space mapping is cited at its first Section 5 mention, `\citep{bandler1994space}` (an existing key; `main.bbl` unchanged).
- `check_voice` lost an empty loop; the number tokenizer also ignores cite keys.

## Verifier

`verify_evidence.py` (v11's, carried forward):
- new `check_v12()`: pins v11's `main.tex` by SHA-256; the set of number tokens in the body equal to
  v11's and the multiset delta equal to `GAINED_V12` / `LOST_V12` (column-width fractions and pt
  lengths are not tokens); the title, author, date-and-DOI and abstract slices byte-identical to
  v11's; `refs.bib` by SHA-256 and every figure file byte-identical; the same citation keys; every
  v11 tabular body present (with the two heading renames and Table 1's listed row edits) and one
  tabular added; the section order; the primer block and "Position of this work" before Method;
  Results before Related Work; Conclusion, Acknowledgments, then the back matter; the relocated
  paragraphs out of §4 and into Results and §2.4; the glosses of proposals 4, 9, 10, 12, 13 and the
  λ / population change.
- re-pinned sentences the review reworded or moved (the counting-solves reconciliation, the
  wall-clock ratio, the mesh-limit drifts, the 9.0 MHz step, the eigenmode of design C, the
  four-parameter design, the probe sentence, the Conclusion's phrasing), and the cost table's rows
  in place of the prose costs, each still recomputed from the records (the descent's
  `total_wall_s` and `n_driven_solves`, the Palace sweeps' `wall_s`, the control's `total_wall_s`,
  the per-solve costs and `wall_s_to_level`; the Palace-driven rows' solve range is checked against
  the seeds' minutes). Both Palace sweep logs are asserted to read "Running with 16 MPI processes".
- `check_v11()` makes its v10 → v11 comparison on v11's own `main.tex`; `check_prior_versions()`
  covers v1–v11.
- `check_voice()` keeps its rules (no "certified", no "solve count", the Conclusion's pinned
  phrases including "re-centred using Palace at third order and a rim-refined mesh") and adds: no
  "times fewer" in the Conclusion, which is under 260 words.

Mutation checks (each reverted): moving a Table 6 row's margin by 0.1 fails `check_headline`;
writing "$386$" in the cost table fails the cost-table row pin in `check_baselines` (and would
fail `check_v12`'s delta); swapping Related Work back before Method fails the section-order pin.

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.12`: 23 pages (v11:
  22; the cost table and the glosses add about half a page), 0 overfull boxes (Table 6's column
  padding is 3 pt, as Table 2's, since "Unknowns" is wider than "DOFs"), no undefined references
  or citations. `main.bbl` is byte-identical to v11's.
- `python3.14 -I verify_evidence.py`: PASS.
- No `[PENDING` marker.
