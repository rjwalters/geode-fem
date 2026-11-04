# Revision 11 → 12

Inputs: `motor-gradient-optimization.11.review/` (`verdict.md`, `review-opus.md` 35/44, `review-fable.md` 36/44; both advance, no critical flags, 0 pending markers) and the coordinator's v12 instructions of 2026-10-21. V11 and earlier versions are unchanged.

**No earlier number changed.** New derived quantities are recomputed by `verify_exp2b.py` from inventory-bound extracts:
- per-start B/F in units and seconds on matched starts;
- matched-start F/A′ and chain closure;
- per-step and per-solve costs and all-target validation shares;
- stop reasons and best-objective agreement.

Results:
- Build: 3 pdflatex passes, 0 overfull boxes, 0 undefined references, no `??`, 0 pending markers.
- `verify.py` (12 PASS blocks, chaining `verify_exp2.py` and `verify_exp2b.py`) passes. `verify_exp2b.py` passes alone (4 PASS blocks) and with the raw re-derivation (5).
- `numeric_consistency`, `pending_marker` and `audience_check` all pass.

**Length.**
- Main text pp. 1–14; the data statement starts p. 15, followed by appendices A–J. Total 34 pages (v11: main text 18).
- Abstract: 241 words (v11: about 330–450).

**L1 FD-accuracy measurement (coordinator item 2): done in a second v12 commit.** The first v12 commit could not run it: `harness/fd_gradient_check.py` hard-coded L0, and adding a level needed a harness change, so I stopped and reported back. PR #1222 then added `--level {0,1}` and the L1 record, merged at `74843242`. V12 now reports that record (addendum below).

| Source | Finding | Resolution |
|---|---|---|
| v11.review merged R1 (Opus R4, Fable R1) | Serial FD stencil; per-core ratio; give no-validation ratios | The abstract says every run is one single-threaded core, so the stencil is solved serially. It gives 5.58/10.07 beside 2.47/4.11. The §5 Protocol states that the time ratio is a compute ratio and that a parallel stencil would shorten wall time but not compute. The §7 "What transfers" paragraph repeats the point |
| merged R2 (Opus R1) | "Gradient access" mislabel | Replaced throughout. The ablation "changes only how SLSQP obtains its gradient, and therefore what the gradient costs"; B/F is "COBYQA against SLSQP driven by finite differences". The verifier asserts that "gradient access" is absent |
| merged R3 (Opus R2, Fable R4/A6) | Per-start B/F; replace log share; matched starts | New §5 paragraph "The optimizer factor" reports matched start sets (5 in space A; 4 in space B, B missing LHS 3), where F/A′ on the same starts is 2.47 and 4.10. It gives per-start B/F: 1.24, 1.45, 0.70, 0.95, 2.27 (gm 1.22) and 3.84, 2.46, 1.26, 1.12 (gm 1.91) in session units; 1.30 and 2.03 in seconds. Chain closure is 3.02/7.82 vs 3.10/7.79. With 8 parameters COBYQA matched or beat FD-SLSQP from 2 of 5 starts. The log-share statistic is removed (verifier asserts "logarithm of" is absent) |
| merged R4 (Opus R3) | "Dilutes every ratio" | Removed. §7 now says validation compresses same-resolution ratios toward one and inflates SLSQP's lead over D |
| merged R5 (Fable R2) | Title evidence only in an appendix | New Fig. 5 (`render_exp2b_figure.py`): per-start charged minutes to the primary target for A′, F and B in both spaces, with the censored B cell marked. Minutes avoid the unit difference. Table 8 stays in Appendix D |
| merged R6 (Fable R3) | FD accuracy at L1; stop reasons | Stop reasons are reported: space A, F 8 budget / 2 converged against A′ 4 converged / 4 solver-stopped / 2 budget; F's best objective is within 0.4% of A′'s from every start. Space B: all 20 at T99. L1 accuracy: measured, see the addendum |
| merged R7 / coordinator 3 | Abstract and length; headline five times | Abstract rewritten to 241 words (Fable A10: opens the ablation sentence "A post-hoc ablation, not preregistered"). The headline appears once each in the abstract, introduction and conclusion; the §5 results give the measurement; the Discussion gives no numbers. The main text went from 18 to 14 pages. Appendices E–H took, unchanged: the forward reproduction detail (estimator controls, reference refinement, Opt120), the Taylor windows, mesh convergence and model dependence, the session and refinement-host detail and the training-gradient numbers (new App. C), and the held-out currents |
| Opus A1 | Title carries the predictable contrast | Title kept on the measured contrast, with Fable A9's exact numbers: "…2.5 and 4.1 times sooner than finite differences…". The decision result (the adjoint, more than the optimizer, accounts for the advantage; FD-SLSQP is not reliably faster than COBYQA in 8-D) is now in the abstract, introduction, §7 and conclusion. The conclusion says "post hoc" |
| Opus A2 / Fable A2 | FD step unit; warm start | §5: "a step of 1e-6 in each coordinate's physical unit (metres for the lengths and offsets)". Each stencil solve is an ordinary training solve on the base's mesh generation (driver `fd_solve`) |
| Opus A3 | Consistency of no-validation ratio and step-cost ratio; adjoint step cost | §5: 5.58/10.07 against step-cost ratios of 5.3/10.9. The exact space-B ratio is 10.9; Opus's 10.7 came from rounded inputs. A stencil solve costs a median 0.23 units, like any training solve, and the adjoint adds 0.06 units per step (about a quarter of a forward), consistent with §6's 1.13–1.20. This corrects the review's inference of about 1.7 forwards per adjoint step |
| Opus A4 / Fable A5 / coordinator 4 | Derivative-method references | Verified via Crossref/OpenAlex/Semantic Scholar with no email or identifier sent (`citation-metadata-v12.json`, `citation-notes.md`). Added Martins & Hwang 2013, Giles & Pierce 2000, Park et al. 1992, Nikolova et al. 2004, Kuci et al. 2017, and Kuci et al. 2016 (whose verified abstract supports the IPM-machine application). Giles & Pierce and Kuci 2017 have no retrievable abstract and are cited only for what their titles state. They are positioned in a new related-work paragraph and in the introduction's cost sentence |
| Opus A5 | Raw traces not public; package unpinned | Partly: the data statement says the raw trees (5,453 and 2,118 files) are retained in two verified copies each, and that the verifiers re-derive the ablation extracts given them. A Zenodo deposit and DOI are left to the operator |
| Opus A6 / Fable A3, A4 / coordinator 3 | Table 9 caption; text-vs-table statistic; Table 10 white space; Fig. 3 ticks | Table 9 caption is self-contained. The text now cites Table 9's all-target medians (61–65% for A′, 16–27% for F). The ablation tables sit together in Appendix D. Fig. 4 (old Fig. 3) has ticks at 1/10/100 with wider panel padding (`render_exp2.py`; the regenerated tables are byte-identical) |
| Opus A7 | Method/space letter collision | Declined. Renaming spaces across text, tables and figures is high-risk for a nit; the text adds "eight-/18-parameter" where it matters |
| Opus A8 | Forwarding cross-reference | Points to Appendix D directly |
| Opus A9 / Fable A8 | Conclusion repeats 3.6×/96×; headline repetition | Conclusion rewritten without those numbers; Discussion refers to §5 |
| Opus D7 / coordinator 3 | Internal word "ruling" | Removed; the protocol timing moved to Code and data, without "ruling" (verifier asserts absence) |
| Fable A1 / coordinator 3 | Stale package README | Rewritten: v12 state line; the "results not yet available" paragraph is gone |
| Fable A7 | Provenance language in body | Moved to Code and data |
| Fable A9 | Title rounding | "2.5 and 4.1 times" |
| verifier | Bindings | `verify_exp2b.py`: new numeric checks; a new phrase list for every new or moved sentence; the title; abstract ≤ 250 words; absence of "gradient access", "ruling", "dilutes every", "logarithm of". `verify_exp2.py`: four phrase bindings follow the new wording (same numbers) |
| package | `.anvil.json` | cwd → `motor-gradient-optimization.12`; commands unchanged (`verify.py`, `verify_exp2b.py`) |

## Addendum: F's L1 gradient check (second v12 commit)

Source: coordinator message, PR #1222 merged at `74843242db03e8d57edf971420171064bf19f107` (verified MERGED and an ancestor of origin/main). The record `benchmarks/motor_optimization/harness/fd-gradient-runtime-checks-L1.json` is copied to `figures/src/data/exp2b/`. It is byte-identical to the PR head. It is in the evidence manifest with that source commit and its SHA-256 (125 entries). The record checks F's own stencil against the adjoint: training on L1 at Newton 1e-8, validation on L2, frozen 1e-6 step, θ0 and the interior point in each space.

| Item | Resolution |
|---|---|
| "Measured at L0 only" statements | Replaced in §5 Results, §7 limits and Appendix D. No longer stated anywhere (README updated too) |
| Result reported | Norm-wise (max-norm) relative error is 5.2e-4 to 1.9e-3 for F and ≤ 7.2e-4 for the mean, all within 5e-3. **Formal component-wise FAIL** at one point: SW3 of the mean gradient at the space-A interior point is −9.62e-3 by FD against −7.74e-3 by the adjoint, relative 0.24 against the 0.1 limit, on a component 8.7e-5 of the largest |
| Interpretation, labelled | Its absolute error, 1.9e-3, matches SW3's at the three passing points (1.8–1.9e-3). The paper reads this as the fixed step's noise floor on a small component and says no step sweep has tested it |
| Verifier | New `verify_exp2b.py` block: manifest pin and SHA-256, session levels and step. It recomputes every norm-wise and component error from the raw FD and adjoint values with the record's own definitions (max-norm; negligible-component rule) and matches the record's stored values and `pass` flags. It asserts exactly one failing component (SW3 dmean, space A interior) with the stated values, and binds every new sentence. The two retired "not measured at L1" phrase bindings are replaced |
| Length | Main text still pp. 1–14; abstract unchanged (241 words) |
