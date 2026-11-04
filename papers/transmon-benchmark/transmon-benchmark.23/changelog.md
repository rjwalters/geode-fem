# transmon-benchmark.23 changelog

v22 → v23 is a reading-flow revision (geode-fem#1231), 2026-10-23, from a flow review of
v22 for an interested non-specialist reader (19 proposals and a glossary) plus the two
deferred #1231 Judge nits. Hard rules kept:
- no number, result, claim scope, title, author or DOI changed; `check_v23()` pins that the
  multiset of numerals in the document body (1,005 tokens) equals v22's exactly, so no
  number was added, dropped or altered, and every new sentence was written without a
  numeral;
- no new reference (same citation set, `refs.bib` byte-identical);
- the voice is unchanged: "bounded", never "certified";
- the abstract is 199 words (cap 200; v22: 199);
- no solver, no `cargo` and no new computation was run; v1–v22 are unchanged.

## Applied

| # | Proposal | Effort | What changed |
|---|---|---|---|
| 1 | geode-fem introduced before the main-result paragraph | S | The GEODE-FEM paragraph moves ahead of "Our main result", opening "Two solvers appear. Palace is the one the example uses; the second is GEODE-FEM ...". Its last sentence stays with the result paragraph, with its referent named: "explains why the same-mesh agreement places the shift in the mesh". The result paragraph now opens "The published frequencies come from one discretization" (its "Those" no longer had a near antecedent). |
| 2 | Abstract labels and baseline for 7.0/8.0 % and 7.3–9.6 % | S | "puts the initial and final designs' qubit frequencies $7.0\%$ and $8.0\%$ above their published values" and "put both designs' qubit and resonator frequencies $7.3$--$9.6\%$ and $2.7$--$3.3\%$ above their published values" (was "higher"). Section 1: "the qubit frequencies of the initial and final designs". |
| 3 | bound/cap/band mapped to the Table 6 tiers; "estimate" added | S | A new Section 1 paragraph: "Five words sort the evidence, and Table 6, in the Discussion, sorts every result by the tiers they name": bound (Bounded), cap (Model-dependent), estimate with its band (Estimated), computed value (Computed). Section 6's "not in the Bounded tier" now says "of Table 6". |
| 4 | Section 8 out of the narrative | S | The whole section moves after Reproducibility and its Acknowledgments paragraph, as the last section, retitled "Author Contributions and How geode-fem Was Built"; its text is unchanged (operator ruling, 150–250 words). `\ref{sec:built}` from Section 3.3 and the Acknowledgments (approved wording, unchanged) still resolve; the Acknowledgments now point forward to Section 11. |
| 5 | Figure 3 next to the paragraphs that read it | M | The float moves in source to just after "A second refinement family" with `[t]`; it renders at the top of page 10, between that paragraph and "Adaptive refinement", and the "Result" paragraph now names the three checks: "a second refinement family and Palace's adaptive refinement (Figure 3; the adaptive runs check only the qubit), and a common-limit fit" (the figure reference sits on the two checks the figure shows; PR #1245 Judge). Figure 2 stays `[htbp]` (it did not drift). |
| 6 | "Palace runs" defined at first count | S | Section 1: "in nine Palace runs (one Palace eigensolve each, the unit in which this paper counts optimization cost, Section 7)". Abstract: "Palace's adaptive refinement, still rising, puts ..." (no collision with the cost unit). |
| 7 | "three Jacobians" pinned to Table 4 | S | Section 7: "with any of the three Jacobians of Table 4 (the shape derivative, and the scaling law with either $\delta$)". Discussion: "converges with any reasonable Jacobian: the shape derivative or the scaling law ..., in 3 or 4 runs (Table 4), and even Broyden's two-probe secant in one or two runs more" (the opener now says "so few Palace runs", so the 3 and 4 are quoted once; Broyden's 5 is Table 4's, written in words to add no numeral). |
| 8 | One name per design; "unmodified device" defined | S | Section 1 defines "which we call the \emph{final} design" where the targets are given. The abstract's first sentence names "an initial design and a final design optimized for $3.0$ and $4.0$\,GHz" (PR #1245 Judge: the targets belong to the final design only); its last paragraph, contribution 3 and the Conclusion say "the final design" (the $3.0$/$4.0$ GHz targets stay in those sentences, as targets). Section 6: "The \emph{unmodified} device is the initial design as published, before the re-optimization of Section 7 changes $c$, meshed as the fixture." |
| 9 | family a / family b, and "ladder" defined | S | "family-a series", "family~a's estimate", "family~a's common-limit fit", "Family~b at step 2 gives", "above family~a", and Table 5's caption "family b by Palace alone". Section 3.1: "a \emph{ladder} is one geometry's series of uniformly refined levels, from $h$ down, at each element order." |
| 10 | "static route" named in the Introduction | S | "(the \emph{static route}, Section 6)". |
| 11 | Extrapolation-rule paragraph unpacked | M | "The \emph{band} is the minimum and maximum of six extrapolations, the same two pairs taken three ways"; the Richardson/GCI comparison becomes a footnote on the rule's first sentence (its wording and numbers unchanged); the closing sentence is "The rule was fixed after that static route was known (Section 1); extrapolation at the observed rate alone would put the initial qubit at $4.63$\,GHz, above the cap, so the observed-rate values enter only as the band's high side." |
| 12 | Abstract claim-then-deny; the FDTD denial | S | Abstract: "Palace reproduces them within $0.7\%$ at the example's own discretization". Discussion: the clause is motivated rather than cut, "Palace is used here as a frequency-domain finite-element eigensolver, so there is no time stepping or numerical dispersion to suspect: it returns the eigenmodes ..." (the paragraph was added in v20 for the coauthor's question, so the point is kept and given its reason). |
| 13 | Meshes behind each same-mesh agreement figure | S | Section 7: "On the ladders of both steps, geode-fem and Palace agree ... to $2\times10^{-4}\,\%$". Discussion: "$2\times10^{-4}\%$, the looser of the ladder and re-optimization values (Sections 5 and 7)". The abstract's "on shared meshes" already covers every shared mesh; Sections 1 and 5 already name the ladders; Table 2's value is stated as the fixture's. |
| 14 | Eigenvalue-derivative paragraph's forward reference | S | Section 3 reordered as 3.1 Device, meshes and materials; 3.2 Junction and circuit models; 3.3 geode-fem, and retitled "Device, Models and Solvers", so Eq. (1) (the port terms) precedes Eq. (2) (the eigenvalue derivative) that cites it. Equation numbers shift accordingly; every reference is `\eqref`. The paragraph's text is unchanged. |
| 15 | The "rule fixed after the static route" caveat | S | In full once, in Section 1; Section 5 keeps the short form with a pointer (item 11); Section 6 reads "a consistency check (Section 1)"; the Table 6 Consistency row is unchanged; the Conclusion drops ", a consistency check rather than a blind prediction" (its "is consistent with that shift and caps the initial qubit under that model" stays). |
| 16 | "Measured" tier vs measurement | S | The tier is renamed \emph{Computed} (Table 6 caption and row, Table 7 row, Section 1's definition), and "measured value(s)" becomes "computed value(s)" in Table 3's caption, Section 5, Section 6 and the Discussion. "measurement" keeps its laboratory sense (Related Work, Table 6 "Not claimed"); "measured" survives only as a verb, twice, in "Threats and limitations". |
| 17 | Jacobian columns signposted | S | Section 7 opener: "Newton's Jacobian has one column per design parameter, $c$ and $L$; the next two paragraphs give how each is obtained." |
| 18 | Which capacitor length is the headline | S | "The headline length we quote is not this design's $c$ itself but the two families' qubit estimates mapped to $3.0$\,GHz by the scaling law $f_q \propto c^{-1/2}$: they call for $c = 1451$ and $1466\,\mu$m, ..." ("this design's $c$" rather than the proposal's "step 2 itself", to add no numeral). |
| 19a | The from-below heuristic moved to the Discussion | S/M (lower priority) | "Why the qubit is approached from below" now follows "Where the shift comes from" and precedes "Why the shape derivative gives no advantage here"; text unchanged. Section 6 ends on the cap. |
| J1 | #1231 Judge nit: the precision argument scoped | – | "On the levels both codes ran, the two coarsest family-a levels with linear elements, the two codes agree on the qubit frequency to $2\times10^{-4}\%$ ...; on those levels, a loss of precision large enough to matter in either code would appear as a same-mesh disagreement far above that level." (The levels are $h$ and $h/2$ at $p = 1$ on family a, said in words so that no numeral is added; the PR #1245 Judge corrected a first draft that said "the two coarsest levels of each ladder", which would have included the $p = 2$, family-b and scaling-law ladders geode-fem never ran.) |
| J2 | #1231 Judge nit: the Figure 1 colour rule | – | "Each label has the colour of the object it names, except the $k$, Vacuum, Sapphire and Metal-sheets labels; the design parameters are annotated in green." ($k$ is drawn in grey and the three (c) labels in black, as the figure source has always done; no figure changed.) |

Abstract, for the 200-word cap: "computed with Palace on one mesh" (was "the Palace solver"),
"We find them discretization-limited", "the independent solver geode-fem agrees on shared
meshes to $2\times10^{-4}\%$ (qubit) and $0.03\%$ (resonator)", "Uniform refinement
converges slowly because of the singular metal edges", "estimates that lean low" and
"Palace's still-rising adaptive refinement". Kept
as pinned: "the fastest rate those edges allow on uniform meshes", "caps the qubit at
$4.477$\,GHz ($+8.1\%$)", "All frequencies are linear eigenmodes", the $19$--$20\%$
sentence and the Newton sentence.

## Skipped

| Proposal | Why |
|---|---|
| 19b: move the bound's verification sentences ("The tests compare it with an exact parallel plate ... far below the rounding") to Reproducibility | Those sentences scope what "holds by theorem" means (exact arithmetic of the discretization, tests rather than formal verification, plain f64 with a stated summation error). Separating the scope from the claim would weaken the bounded-not-certified discipline at the one place the paper states a bound; they stay beside Eq. (4). |
| 4, split option (first sentence into Section 3.1, the rest after the Acknowledgments) | The whole-move option was taken instead: the operator ruled the section's content as one 150–250-word unit, and Section 3.3 already cites it for the specification. |
| 12, cut option for the FDTD clause | Motivated instead of cut (see item 12). |
| 8, "the 3.0/4.0 GHz design" kept for the abstract and conclusion | Not kept; "the final design" is used, with the targets stated as targets in the same sentence. |

## Verifier

`verify_evidence.py`:
- `check_v23()` (new): v22 `main.tex` pinned by SHA-256 (also added to the v10–v21 list);
  the multiset of numerals in the body and in the preamble equals v22's (regex
  `-?\d+(?:\{,\}\d+)*(?:\.\d+)?`, 1,005 body tokens); the `\title`, author block and
  date-and-DOI block are v22's; the citation set and `refs.bib` are v22's; the four-line
  header comment names v23; the section and subsection order; the position of every moved
  block (geode-fem paragraph, Figure 3 `[t]`, the heuristic, Section 11) and Eq. port before
  Eq. hf-eig; the GCI footnote; one copy of each applied wording; no "first/second family";
  the Computed tier with "measured" only as a verb; no "$3.0$/$4.0$\,GHz design"; the stale
  phrases absent; "certif" absent; the abstract at most 200 words without "consistent".
- Updated where the text moved or was reworded: the rule-fixed caveat (Section 5), the
  abstract sentences, "computed value", family a, the three Jacobians, Table 6/7 rows, the
  section-order check (`check_v16`, now `order[-4:]`), the bound/cap/band definition
  (`check_v19`), the "Where the shift comes from" slice and its sentences and the Figure 1
  caption (`check_v20`), the Acknowledgments slice (ends at Section 11), and `check_v22()`
  now compares v22 with v21 on v22's own `main.tex`.
- Mutation check: changing one digit of the initial resonator's published value in Section 1
  ($5.591 \to 5.519$) fails `check_v23()` on the numeral multiset (reverted).

`verify_figures.py`: the Figure 1 caption pin carries the exemption; the figures are v20's
bytes, so every byte and regeneration check is unchanged.

## Build and checks

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.23`: 21 pages (v22: 21),
  0 overfull boxes, no undefined references or citations; `main.bbl` byte-identical to v22's.
- `python3.14 -I verify_evidence.py`: PASS.
- `python3.14 -I verify_figures.py`: PASS.
- No `[PENDING` marker.
- Pages 2, 10 and 19 rendered and inspected: the solver paragraph precedes the result
  paragraph and the tier paragraph follows it; Figure 3 heads page 10 between the family-b
  and adaptive paragraphs; Conclusion, Reproducibility, Acknowledgments and Section 11 close
  the body.

## For the operator

- Independent v23 reviews are pending.
- `papers/README.md` is not updated here (parallel PRs; operator's call).
