# Proposed amendment 8: a coarse nonlinear physical baseline

2026-10-18; proposed by Codex for independent review by Claude, before any main run. This augments #1185 and amendment 7. It changes neither the recorded pilot nor the common targets, anchor, error margins, spaces, geometry domain or validation policy. Adoption and the exact implementation commit must be recorded on #1185 before main outcomes.

## Question and model

Can an inexpensive nonlinear physical model reach the same validated targets as the L1 exact-gradient method within the same measured wall-time budget?

Use the existing **L0 nonlinear FEM model** with the same geometry parameterization, material table, physical current, torque estimator, harmonic maximum h45 and twenty training angles as L1. Only spatial resolution changes. Keep all three training current fractions (0, 0.5, 1) and the fixed-size/control settings of the main study. The scalar-only and scalar-plus-ten-group spaces are unchanged. Meshes use the same movement, quality and deterministic repair rules, at the chosen resolution.

This is a coarse physical model, not a fitted empirical surrogate, magnetic-equivalent circuit, linear magnetic model or reduced-basis model. No claim of comparison against every inexpensive model is allowed. The two-design screen in `coarse-physics-screen.json` motivates this choice; it supplies no main-run data or performance estimate.

## Methods and paired comparisons

Retain A (safeguarded SLSQP on L1), B (COBYQA on L1) and C (empirical RBF trained on L1). Add:

- **D: COBYQA on L0.** Same pinned COBYQA version, unit-box scaling, initial/final trust radii, rejection mechanism, constraints, callback policy and stopping rules as B. Forward values only. This is the primary inexpensive physical competitor without exact derivatives.
- **E: safeguarded SLSQP on L0.** Same exact-adjoint callback policy, tolerances, admissible-anchor restoration/restarts and stopping rules as A. This separates the value of derivatives from the choice of spatial fidelity.

The four FEM methods form a 2×2 comparison of optimizer and spatial resolution. Compare A/B and E/D for optimizer differences at fixed resolution, A/E and B/D for resolution differences at fixed optimizer. A/D alone changes both factors and cannot identify which one explains a difference. C remains a separately labelled empirical competitor.

All five methods run in both spaces at the same five frozen design starts with two timing repeats: **100 main runs**. Timing repeats are not independent designs. For start index s=0,…,4, rotate ABCDE left by s on repeat 1; use the reverse of that exact order on repeat 2. Record the complete execution order. Do not choose a method or omit E after inspecting outcomes.

## Calibration, constraints and common validation

Compute the training objective and full-current mean at theta0 separately for each resolution. As for the existing methods, normalize each model's objective and training mean constraint by its own theta0 values; the training requirement is mean/mean(theta0) ≥ 1.0025. These model-relative constraints may differ in physical value. They guide the search but never replace the common validation mean floor.

There is no fitting, bias correction, cross-level gradient substitution, adaptive refinement or use of validation outputs to tune the low-fidelity model. Freeze any later change through a new amendment before its outcomes. Record one-time reference computations, meshing and setup costs by resolution, with shared and method-specific costs separated. Report end-to-end totals including those costs as well as the registered per-run comparison; do not hide a new setup cost by calling it shared calibration.

Use the same L2/h93/g20 validator, exact anchor hash, T50/T90/T99 thresholds, delta-F rule and mean floor for A–E. Validate new accepted incumbents and final candidates according to the existing method callback policy, with the corrected initial-set gate and no duplicate/start validation. Only completed validations within budget earn credit. A candidate must also satisfy the common geometric admissibility constraints and mesh policy to count as feasible or attain a target; an inadmissible low-F point earns no credit even if an optimizer reports it as its best point.

All five starts must pass the adopted domain and fresh-mesh policy at both L0 and L1 and in both spaces before the common list is frozen. Preserve already valid pilot starts; use amendment 7's deterministic replacement rule otherwise. Publish every replacement index and reason. No start selection may depend on objective or method outcomes.

## Cost and termination

Every run receives 120 times the same cold validation-forward wall-time unit, with the same 60-unit checkpoint. Charge actual wall time for model initialization, geometry checks, mesh repair, forward/adjoint solves, line searches, restarts, validation and driver work. L0 receives no artificial cost discount: its cheaper solves can buy more requests within the same wall-time budget. Record raw seconds, solve counts, normalized cost and failures. Do not reuse another method's training cache or trajectory; identical cache policies remain available within each run.

Apply amendment 7's host-contamination rule to all methods and preserve contaminated records and clean reruns. Use common CPU/thread limits. If a method terminates without a target, report that outcome; do not restart it selectively beyond its registered safeguard. No finite speedup ratio is inferred for an unattained target.

## Model fidelity and final-candidate checks

For D/E, retain the paired L0 and common-validation objective, per-current standard deviations and full-current mean at every validated design. Compare all such records, including failed acceptance, rather than only the winning point.

After each run, choose its best common-validated feasible candidate (lowest F_val; earliest validation breaks a tie). If there is none, use its last buildable training incumbent and label it as lacking validated feasibility; if no such point exists, record the diagnostic as unavailable. At that frozen design, evaluate L0 and the common validation model at fractions 0.25 and 0.75, with twenty angles. The L0 model solves the nonlinear equations at those currents; it does not interpolate between fitted current outputs. Record mean, standard deviation and waveform differences. These are off-training-current fidelity checks, not learned-model generalization.

Record those post-run diagnostic costs separately. They do not change the stopped trajectory, target credit or timing. Final selected candidates from each method/space still require the retained L3/g40 and interface checks. A claimed optimization benefit must survive those checks or be qualified by their observed discrepancy.

## Implementation and review gates

Before main runs, merge an independently reviewed implementation that:

1. Records training resolution, source/mesh hashes, material/current/angle settings and requested/actual design coordinates in every run; asserts the common validator and anchor are identical across resolutions.
2. Exposes a real L0 training configuration without silently substituting an L1 mesh. Tests that changing training resolution leaves validation unchanged.
3. Checks L0 adjoint derivatives for the actual scalar and grouped-control problem, at admissible frozen points, against a step-size sweep of directional central differences. These checks are setup evidence, not optimization timing. Retain failures; do not tune on main outcomes.
4. Reuses the reviewed amendment-7 safeguards and initial-set gate, includes all five methods in scheduling, ledger, failure and checkpoint summaries, and tests target rejection for inadmissible candidates.
5. Commits all starts, exact model constants and options, scheduling order and command lines before any main outcome. A bounded smoke run verifies wiring only and is labelled as such.

Claude independently reviews this protocol before adoption. Main execution also needs the reviewed safeguard implementation and the previously specified host choice. Protocol acceptance alone establishes no numerical result or publication claim.
