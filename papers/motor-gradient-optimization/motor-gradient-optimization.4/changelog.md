# Revision 3 → 4

The operator requested continuation after the v3 review/audit passes and new reviewed experiments. V3 remains unchanged; its actual Opus/Fable reviews and audit are copied byte-for-byte into `.3.review/` and `.3.audit/`. This is an operator-requested evidence expansion, not a claim that v3 failed. V4 requires fresh reviews and audit.

| Source | Note | Resolution |
|---|---|---|
| .3.review Opus S1 / generic | Three reversals overgeneralized | Abstract/conclusion now name initial L1, order-45, full-current state; no universal count claimed. |
| .3.review Opus S2 / generic | Compare gradient gaps against refinement | Recompute pinned L3 gradients: ripple gap 6.58/10.25% versus within-model changes 0.46–1.19%; mean gaps shrink to 0.16/0.26%, comparable with conforming refinement. Verifier reconstructs these independently. |
| .3.review Opus S3, Fable v2 S3 / generic | Opt120 deferral | Superseded by reviewed #1182 evidence. Added full exported-design validation with estimator-domain and current-rule distinctions. No invalid full-band-Arkkio result is used. |
| .3.review Opus S4, Fable S1 / generic | Full-order abstract ambiguity | Explicitly retain every order below trace Nyquist, rather than implying every ladder entry agrees. |
| .3.review Opus S5, Fable S3 / generic | Norm hides component differences | Name initial SW2/SW3 mean discrepancies (6.5/17%) and retain MAG ripple examples and component plot. Decline Fable's 'every mean ratio within 0.84–1.11': near-zero components and the SW3 discrepancy make blanket ratios misleading. No cosine is needed for the claim. |
| .3.review Opus N1 / generic | Geometry annotation on air-gap band | Move annotation outside the plotted geometry; check rendered arrow/text separation. |
| .3.review Opus N2, .3.audit figure note | Legend/annotation overlap in sensitivity bars | Move current-policy legend outside axes and +1.68 label into empty space. Inspect the new raster and compiled page. V3 preflight's no-overlap statement was incorrect; do not carry it forward. |
| .3.review Opus N3/N4, Fable S2 / generic | Missing signs | State loaded std decreases and cogging std increases, consistently in abstract, introduction, results and conclusion. |
| .3.review Opus N5 / generic | Identify Taylor-window failures | Name conforming/fixed-density SW1, initial/full current, L0/L1. |
| .3.review Opus N6, Fable S9 / generic | Empty author | Robb Walters per operator guidance relayed in Squad 451; also set PDF author metadata. |
| .3.review Fable S4 / generic | Taylor normalization/guides | Caption names division by absolute base objective and dotted h² guides. |
| .3.review Fable S5 / generic | Gray connectors unexplained | Caption identifies each model-estimator pair's connector. |
| .3.review Fable S6 / generic | Duplicate percent signs | Keep unit in table header only. |
| .3.review Fable S7 / generic | Stacked current-state qualifiers | Use a compact parenthetical state description. |
| .3.review Fable S8 / generic | Claimed first-order SW3 convergence | Replace with the measured fact that increments shrink. |
| .3.review Fable S10, Opus D4 / generic | Position against harmonic-coupling work | Read primary PDFs. Egger §5 already varies cogging harmonic order; Bontinck §3 discusses multiplier/field-space stability; Merkel validates EMF shape optimization. State what the present nonlinear torque/candidate/gradient comparison adds without claiming discovery of truncation effects. |
| .3.review Fable S11 / generic | Solver-verification detail interrupts flow | Condense to the checked problem classes and source-record link. |
| .3.review Fable layout nits / generic | Legend/page spacing | Place derivative-cost legend between curves; rebuild and inspect all figure pages. Retain slotless appendix as a bounded screening control, not optimizer evidence. |
| .3.audit numeric note 1 | Optional 1.3e-9 → 1.2e-9 | Declined: raw max 1.2417e-9 makes 'within 1.2e-9' false. Preserve true 1.3e-9 upper bound and the existing verifier guard. |
| .3.audit citation note | Title-case ICEM title | Normalize to sentence case; reference identity unchanged. |
| .3.audit numeric hardening | New claims need raw checks | Extend independent stdlib verifier for reference reruns, current rules, interface/grid changes, invalid mesh and L3 gradient norms. |
| Reviewed #1191 | Cogging cause unresolved | Reproduction at 3×3 precedes refinement; 24×24 agrees to 0.10%. Retain residual reference-refinement limits and distinguish cogging extrapolation from loaded increments. |
| Reviewed #1182 | Candidate-resolution evidence | Use fresh export/NURBS subset. L2→L3 changes in this subset are ≤1.36% initial, ≤2.71% Opt120; broader-source summary maxima are not substituted. L3 g10 errors are +1.05%, −5.90%, −17.1% for h45/h93/conforming. |
| #1185 amendments 1–4 | Prospective comparison | Record fixed 8/18 spaces, physical current, predeclared target ladder, candidate-based allowance, full cost accounting and censoring. Label RBF as empirical; no smoke/pilot outcome enters the paper. |

## Added evidence and figure

112 SHA-pinned inputs, with source commits in `evidence-manifest.json`. The new four-panel validation figure combines independent reference refinement with saved-design interface/current-policy comparisons. It is generated from raw torque records. The older five figure subjects are retained and their captions corrected. No new FEM solves were performed for this manuscript revision.

## Remaining limits

No matched optimizer time-to-target result, no physical motor measurements, no continuum error bound, no broadly validated inexpensive-physics baseline. New candidates still require independent spatial/angular/current checks. Those limitations are not replaced with hypothetical results.
