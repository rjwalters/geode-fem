# Revision 5 → 6

V5 is preserved unchanged at `76d78e1f373b20d0c9c7920582b5a0ddd71082c2`. Original reviews (Opus 42/44, Fable 39/44, both ADVANCE) and the failed audit are archived verbatim in sibling `.5.review/` and `.5.audit/` directories. V6 follows the ongoing operator instruction to improve the paper and Squad request 13; the configured cap is extended by one revision. Fresh audit is required. No new field solve, evidence snapshot or optimization result is added.

| Source | Finding | Resolution |
|---|---|---|
| Audit C1, critical | SW3 difference incorrectly rounded to 17% | Reconstruct 16.493% relative to conforming from L2 initial full-current arrays; display 16.5%. Verifier checks SW2 and SW3 calculations and text. |
| Audit C2, critical | L1 Taylor failure incorrectly attributed to sign change | Distinguish L0 sign change near the anchor from L1 drift in remainder/h². Verifier reconstructs signed remainders and the 2.64/0.459 neighbor ratios, and checks text. Existing 530/592 and 388/592 counts remain unchanged. |
| Opus m1 / audit advisory | Vector change confused with norm change | State 1.19% vector change relative to L2 norm and 0.19% norm decrease; independently reconstruct both. |
| Opus m2 | Budget wording includes excluded diagnostics | Use “All in-run costs”; post-run diagnostics remain separate. |
| Opus m3 / audit citation partial | “Developed by” implies priority beyond source support | Replace with the narrow “formulate harmonic weighting functions” statement; update citation notes. No new source-support claim. |
| Opus m4 / Fable M1 | Waveform supplement inaccessible from manuscript | Include existing two-panel Opt120 waveform as Fig. 5 with current, mesh, angle, estimator and centering conventions; cite it in §5.5. |
| Opus m5 | Nonlinear and spatial convergence conflated | Say “Newton-converged” model and “spatial-refinement sequence.” |
| Fable M2 | Only one rule normalized for size | Add fixed physical 1.4291 N m (−1.0%) and density 1.7061 N m (+18.2%). The latter corrects the review's 1.7062 rounded-input calculation using full-precision arrays. Conclusion attributes most, not all, of the fixed-current deficit to the explicit size factor. Preserve algebraic/non-optimized qualification. Verifier covers all rules. |
| Fable m1 | Fig. 4 far from first reference | Move its environment immediately after the first cited paragraph and allow here/top/bottom placement. |
| Fable m2 | Hard-coded section number | Add section label and use a reference. |
| Fable m3 | Overloaded §5.5 paragraph | Separate interface result, refinement and conjecture. |
| Fable m4 | Matched-current counterexample absent from abstract | Add mean within 0.1% versus 2.0-fold ripple discrepancy, explicitly full current. Half-current ripple is not twice the reference. Verify from raw arrays. |
| Fable m5 | Zero-current source unclear | Specify rule independence and fixed-current record source. |
| Fable m6 | Length-only normalization lacks context | Add +13.9% and retained s² = 1.098, verified from scalars and raw torque. |
| Fable m7, optional | Suggest under-resolved Opt120 reference as cause | Declined: keep cause unresolved until candidate reference refinement supplies evidence. |
| Fable m8 | Discussion repeats headline values | Remove repeated 3.6-fold and size/current statements from discussion opening. |
| Fable m9, optional | Suppress heuristic numeric warning | Retain the visible warning and explicit preflight adjudication; raw torque checks guard the correctly scoped 0.27% claim. |
| Fable m10, optional | Fold slotless appendix into discussion | Declined: keep the distinct negative control with its assumptions and held-out-angle protocol together. |
| Opus initial-family nit | 1.36% is export-family maximum | Explicitly name the exported mesh family. Do not add the unbundled initial construction record for an optional comparison. |
| Opus paired-estimator nit | Add relative std difference | Retain absolute paired comparison; its two estimators, same field, and limited interpretation are already explicit. |
| Opus Fig. 3 nit | Crossing traces near 12% | Retain data positions and established styles. No axis offset; existing current labels and legend identify the curves. |
| Opus compact-table suggestion | Dense size/policy prose | Keep a single comparative paragraph with all three normalized means rather than another table duplicating Fig. 4d. |

## Verification scope

115 pinned evidence inputs and 24 regenerated figure/table/statistics assets; seven figures appear in the manuscript. Author verification reconstructs raw numerical claims and checks the committed archive. It does not substitute for independent audit or establish optimization speedup.
