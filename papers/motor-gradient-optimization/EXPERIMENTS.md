# Experiments requested for the next motor-paper iteration

Requested 2026-10-17 after v2's evidence audit. Coordinate execution in Squad with Claude; #1179/#1180 already own grouped geometry and derivative validation. Do not duplicate their runs. These are proposed measurements, not completed results or preselected favorable outcomes.

## Current evidence and execution status (v6)

- Experiment 1 merged as #1182 at `224332c526be1191183af82b610973c7778de720`. V6 retains fresh export/NURBS L2–L3 records, with coupling torque for harmonic fields and outer-band Arkkio only for the checked fresh conforming meshes. Fixed unscaled and physical current are distinct at Opt120's radial scale.
- Experiment 3 merged as #1191 at `8acd21ecf85d89fe077275a3c04ca70b53963d43`. The coarse rerun reproduces all 60 saved torques before refinement; 24×24 brings cogging std agreement to 0.10%. This resolves the coarse-reference explanation at matched order, without claiming a continuum bound.
- Experiment 2's authoritative preregistration is [#1185](https://github.com/rjwalters/geode-fem/issues/1185), amendments 1–6 and the amendment-6 addendum superseding conflicting original clauses. Its frozen spaces are 8 scalars vs 8+10 offsets, with LENGTH/SD1/ScaleR/OPERATING_ANGLE fixed and physical current 9.6777 A. Targets T50/T90/T99 are set before method outcomes. The budget is 60 measured cold validation-forward wall-time units; all in-run method costs are charged. Reference calibration shared identically across methods is recorded separately.
- Remesh/torque guards and validation infrastructure (#1190/#1192/#1194) are merged; the optimizer harness #1196 merged at `89e6c4e127a5fbef7e4b2e0f10b90e70bd1193b4`. The 36-run development-host pilot is complete in PR #1200, head `66de22d4be1563a90e0dbe9a7a74caeab406dab8` (under review). It recorded 13 failures (11 unbuildable slot flanks, two fresh meshes below the 5-degree minimum-angle policy), 13 budget stops and 10 T99 stops. Space-B T99 attainment is 6/6 SLSQP, 4/6 COBYQA and 0/6 empirical RBF surrogate; space A has no T99 attainment. Counts include earlier target crossings in runs that subsequently failed; final run status remains separate. These loaded-host observations are feasibility/budget evidence only. A prospective geometry-validity and budget amendment is being discussed; it is not adopted here, targets remain frozen, and main runs still require an idle-host session. No smoke result is paper evidence. The empirical cubic-RBF surrogate must be named as such, not a reduced physical model. Its half-current prediction is at a training current. Post-run .25/.75 diagnostics evaluate a frozen linear interpolation between current-specific RBF predictions; their separately recorded costs do not affect the stopped run or attainment times. These test the added current interpolation, not intrinsic current generalization.

The original experiment requests below retain the wider research scope. Where they conflict with the frozen pilot, the issue's amendments govern that pilot. The strongest inexpensive-physics comparison and broader geometric feasibility constraints still need explicit evidence before the full research hypothesis can be claimed.

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

V5 normalization/reproduction correction: removing both explicit size factors gives Opt120 1.4971 N m; length-only normalization to 100 mm gives 1.6434 N m. Matched reference-current GEODE reduction is 10,170-fold versus 11,982-fold saved; the 7,631-fold reduction uses fixed unscaled current. Reference refinement at Opt120 remains open.

## Next substantive manuscript revision

The independent v6 audit passed at `1dd8cdef` with no critical flags and no re-review required. Preserve that version. Carry forward audit N1: change Fig. 5's mean-effect sentence to emphasize resolved ripple and means agreeing within 6e-5 N m; add a raw-array verification guard. Main-experiment results, once validated, require a new version and fresh review/audit. The pilot does not discharge the inexpensive-physical-model baseline requirement.
