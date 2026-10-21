# Revision 13 → 14

Inputs: `motor-gradient-optimization.13.review/` (`verdict.md`, `review-opus.md` 39/44, `review-fable.md` 40/44; both advance, no critical flags) and the coordinator's v14 instructions of 2026-10-21. These include the operator's decision on required finding 1: a second post-hoc arm, method F_w with warm-started stencils, preregistered as issue #1224. V13 and earlier versions are unchanged.

**Title: provisional.** The current title is kept for now ("Per core, adjoint gradients reach validated torque-ripple targets 2.5 and 4.1 times sooner than finite differences in a nonlinear motor model"). Its numbers are to be revisited once F_w's results exist: against warm-started stencils the adjoint's ratios will be smaller.

**No earlier number changed.** New derived quantities are recomputed by `verify_exp2b.py`:
- the per-gradient ratio chain (33/70, 7.4/14.7, 5.3/11.5), from both the data and the printed inputs;
- the per-arm training-solve medians;
- the cold-start record in all 40 traces;
- Appendix H's 0.30–0.40 warm-sweep fraction;
- the path agreement up to each crossing.

**Outstanding dependencies (hold READY):**
- `[PENDING exp2c-fd-warm]` (F_w, issue #1224), 9 markers:
  - abstract (main.tex L35);
  - introduction (main.tex L69);
  - §7 (main.tex L374);
  - Conclusion (main.tex L414);
  - five in the §5 "Warm-started stencils (method F_w)" paragraph (optimization.tex): per-start F_w/A′ ratios, F_w vs F, per-gradient and warm-vs-cold stencil cost, L1 warm accuracy, refinement.
- `[PENDING zenodo-doi]` (data statement): the operator's deposit.

The verifier allows exactly these two sources. Run `grep -n PENDING main.tex optimization.tex` for exact lines.

**Results:**
- Build: 0 overfull boxes, 0 undefined references, no `??`.
- `verify.py`: 14 PASS blocks. `verify_exp2.py`: 4. `verify_exp2b.py`: 6. `verify_exp2b.py` with the raw ablation copy and the archive: 8.
- `numeric_consistency` and `audience_check` pass. `pending_marker` reports `exp2c-fd-warm` and `zenodo-doi`.
- Main text: pp. 1–14, data statement from p. 15. Total 36 pages. Abstract: 236 words.

| Source | Finding | Resolution |
|---|---|---|
| v13.review R1 (Opus R2, Fable R1) / operator | F's stencil solves start Newton-cold | Stated plainly with the trace evidence: §5 Protocol says each stencil solve, like every training solve in the preregistered configuration, starts Newton-cold (all 40 traces record a cold start; a new `training_warm` field in the extract, re-derived from raw). The abstract, introduction, §7 and Conclusion carry the condition. New §5 paragraph "Warm-started stencils (method F_w)": F's cold stencils make it weaker than an FD-SLSQP that warm-starts, with App. H's warm sweeps at 0.30–0.40 of a cold forward. It gives F_w's protocol, labelled post hoc, with a link to the public #1224 protocol frozen before any run. `[PENDING exp2c-fd-warm]` markers go where its results and the gradient-credit sentences will go |
| v13.review R2 (Opus R1) | "Same path" is wrong at space-B LHS 2/4 | Replaced. Both arms cross each target after the same number of validations. Up to the primary crossing, validated objectives agree within 0.4% (space A) and 0.9% (three space-B starts). At space-B LHS 2 and LHS 4 the iterates drift apart before the T99 crossing (29% at the crossing validation, 18% one before it; LHS 2 uses one training evaluation fewer), yet the crossing comes at the same validation. The "after the crossings" text is now limited to space A (detail in App. D). **The phrase binding is replaced by a test of the claim itself**: `verify_exp2b.py` computes the per-start agreement up to each crossing. It asserts the space-A and three-start bounds, that the LHS 2 maximum is at the crossing validation and the LHS 4 maximum one validation before it, the 9-vs-10 training evaluations, and that all space-B runs stop at T99. It also asserts that "take the same path" is absent |
| v13.review R3 (Opus R3, Fable R2) | Per-gradient ratio unprinted; both factors unnamed | §5 prints the per-gradient ratios (33 and 70). It names the shared training solve, with per-arm medians F 0.240/0.236 and A′ 0.232/0.245, which brings the per-evaluation ratio to 7.4 and 14.7. It names the gradient-request share (68%/77% for F; A′ requests at every evaluation), giving an expected 5.3 and 11.5 against the measured no-validation 5.58 and 10.07. §7 names both factors and shared validation. The verifier recomputes the chain from the printed inputs and checks that those inputs are the data's own medians |
| Opus A1 | "Upper bound on the noise contribution" unproven | Replaced: noise at that step "is of that order or smaller, unless it cancels against the remaining truncation term" (the verifier asserts "upper bound on the noise" is absent) |
| Opus A2 | Units on App. D values | Slope and curvature now in N m/m and N m/m², explicitly for the mean gradient |
| Opus A3 / Fable A4 | Fig. 5 censored marker; LHS 4 label | The censored B cell is drawn at B's 120-unit budget and the caption says so. Ratio labels sit in a fixed row at the top of each panel, one per start column, so none is ambiguous; the caption names the row |
| Fable A3 | Split the data statement | Three paragraphs: forward evidence; main study and package; ablation and archive. The middle paragraph is set `sloppypar` to break the package path without overflow |
| Opus A5 / Fable A1 / coordinator (first v14 commit) | Yu et al. 2018 | Verified on Crossref and OpenAlex (no identifier sent; `citation-metadata-v14.json`). No abstract is available on either. OpenAlex's machine keywords include gradient-based and derivative-free optimization, but **whether the paper contains an adjoint-versus-finite-difference arm could not be confirmed**, so it is cited only for what its title states (algorithm and initial-design effects in wing shape optimization) and not as a prior measurement of the contrast. Lyu, Xu & Martins 2014 is still not added (no record) |
| Fable A5 | Two budget-truncated gradients | App. D says the last gradient of each space-A θ0 run of F was cut off by the budget and is excluded from the cost medians |
| Opus A8 | Abstract ending | The abstract now ends on the decision result |
| Opus A7 | Fig. 6 placement | Width 0.88\linewidth; it now sits within §6's pages |
| Opus A4 | Package pointer not on the default branch | Pointer updated to `.14` and bound in the verifier. Pinning to a merge commit or including the package in the Zenodo deposit is left to the operator/coordinator |
| Opus A6 / Fable A2 | Letter collision | Declined again |
| length | Keep ≤ 14 pages | To make room for F_w, moved rather than deleted: Table 3 (main-study validation charges), the space-A post-crossing detail, the A′-vs-A control check and the ablation validation-share sentence went to App. D. Removed: a §7 sentence that repeated the Conclusion and the Conclusion's scope sentence that repeated §7. Yu's sentence and the Conclusion opening were shortened |

## Addendum: Yu et al. 2018 positioned from the full text (second v14 commit)

Source: the coordinator's full-text check of the author preprint, `../refs/yu2018-fulltext-check.md` (committed with this change).

| Item | Resolution |
|---|---|
| Positioning | §2 now reads: "On an eight-variable wing twist problem, Yu et al. (2018) found gradient-free optimizers 4 to over 100 times as expensive as adjoint-driven ones at the same optimum, the counterpart of our eight-parameter SLSQP-against-COBYQA ratios of 3.10–3.25. They do not compare adjoint with finite-difference gradients, which our ablation does, but state its premise: finite-difference cost grows with the number of variables and adjoint cost does not." |
| Our ratio | The space-A B/A geometric means (3.10 at T90, 3.25 at T50) are recomputed by `verify_exp2.py` and asserted again beside the citation in `verify_exp2b.py`; the sentence is bound |
| Lyu, Xu & Martins 2014 | Dropped as a lead: Yu et al. 2018 is its journal successor (same authors, same CRM study) and the 2014 paper has no Crossref/OpenAlex record (`citation-notes.md` updated) |
| Length | Main text still pp. 1–14; abstract unchanged (236 words) |
