# Experiments requested for the next motor-paper iteration

Requested 2026-10-17 after v2's evidence audit. Coordinate execution in Squad with Claude; #1179/#1180 already own grouped geometry and derivative validation. Do not duplicate their runs. These are proposed measurements, not completed results or preselected favorable outcomes.

## 1. Validate the optimization target before timing methods

**Question:** Do improvements survive angular, spatial and interface refinement, including a feasible remesh?

Use the initial design, reconstructed Opt120, and the first new feasible candidate once available. Keep the exact vector, current policy and torque estimator in every record. Start with L2/L3, harmonic maxima 45 and 93, and the existing conforming/all-mode comparison where affordable. Compare 10, 20 and 40 distinct angles over the same period, plus a shifted validation grid (e.g. 20 angles shifted by 0.5 degrees). Check that all selected rotations are supported by each conforming mesh. Evaluate training currents 0, 0.5 and 1 and withheld fractions 0.25 and 0.75. Fixed-current is the primary new shape study; fixed group-1 density is a separately labeled reference-reproduction control.

Export per-angle torques and aggregate mean, standard deviation, ripple-squared objective, constraint values, minimum element quality and invalid-element count. At a candidate with invalid moved elements, remesh and record that cost and topology change; never silently discard the candidate or report its invalid-mesh field. Add paired coupling/Arkkio torque on the same harmonic field, reusing existing controls where possible.

**Decision:** Freeze an error margin and feasibility margin from these refinements before comparing optimizers. An improvement smaller than observed refinement changes is unresolved. If the 45→93 change reorders candidates, report model dependence and use a common refined evaluation for all methods.

## 2. Measure useful design freedom and time to target

**Question:** Does the verified gradient reach the same validated target faster than the strongest inexpensive alternative?

After the grouped-control gradient review, compare scalar-only and scalar-plus-group spaces with fixed envelope, axial length, control policy, magnet resource and geometric feasibility constraints. Record which variables remain free under these constraints. The reference groups are 10 group variables acting on 55 points, not 55 independent variables. Infer no novelty from combining scalar and surface controls.

Within each space use matched starts and budgets for exact-gradient optimization, a calibrated waveform/reduced physical model, and a competitive derivative-free trust-region method. Central differences are a derivative-cost diagnostic, not the sole performance baseline. Before runs, commit starts/seeds, target/error margins, bounds, stopping rules and validation rules. Start with a small pilot; preserve negative and infeasible outcomes. Choose the target before inspecting method outcomes.

Charge setup, calibration, mesh motion/remeshing, field/adjoint solves, line searches, validation and surrogate updates. Record wall time, solve counts, achieved feasible objective and time of first validated target attainment. Fix CPU, solver/library thread limits and process affinity where supported; run sequentially on an otherwise idle host, alternate method order and repeat. Permit comparable caching/warm starts to all methods. Report failure within budget without inventing a speedup ratio.

**Decision:** A speedup claim requires a common attained target with validation margins, not just fewer derivative evaluations. Report quality-versus-cost curves if targets differ or are unattained. The reduced-model baseline should be assessed on unseen designs and currents, not only held-out angles at one geometry.

## 3. Resolve the residual cogging discrepancy (parallel, bounded diagnostic)

**Question:** Is the residual 12.9% cogging-amplitude gap dominated by the saved reference's coarse discretization or by GEODE geometry/field error?

If the reference runtime is available, rerun the initial zero-current waveform at 3×3, 6×6 and 12×12 subdivisions per patch, keeping degree, harmonic orders and torque definition fixed. Archive exact runtime versions and inputs. In GEODE, compare straight-sided P1 with a curved/higher-order representation on a comparable mesh if supported. Keep harmonic order 45 so interface truncation is not another changed factor.

**Decision:** Compare full waveforms and refinement increments, not only means or one favorable angle. If the runtime or curved-element route is unavailable, leave the cause unresolved and report the measured discrepancy as a limitation. This diagnostic need not delay setting up experiment 1, but its unresolved status must remain visible.

## Required handoff for every experiment

Pinned commit, runnable command, environment/thread metadata, exact design vector and current rule, raw JSON/CSV including failures, summary script, wall-time breakdown and a short interpretation that separates observation from hypothesis. Figures should be generated from those raw records. Ask for fresh scientific review before incorporating new solver claims into a completed manuscript version.
