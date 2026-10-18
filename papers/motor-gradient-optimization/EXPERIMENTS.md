# Experiments requested for the next motor-paper iteration

Requested 2026-10-17 after v2's evidence audit. Coordinate execution in Squad with Claude; #1179/#1180 already own grouped geometry and derivative validation. Do not duplicate their runs. These are proposed measurements, not completed results or preselected favorable outcomes.

## Current evidence and execution status (v6)

- Experiment 1 merged as #1182 at `224332c526be1191183af82b610973c7778de720`. V6 retains fresh export/NURBS L2–L3 records, with coupling torque for harmonic fields and outer-band Arkkio only for the checked fresh conforming meshes. Fixed unscaled and physical current are distinct at Opt120's radial scale.
- Experiment 3 merged as #1191 at `8acd21ecf85d89fe077275a3c04ca70b53963d43`. The coarse rerun reproduces all 60 saved torques before refinement; 24×24 brings cogging std agreement to 0.10%. This resolves the coarse-reference explanation at matched order, without claiming a continuum bound.
- Experiment 2's authoritative preregistration is [#1185](https://github.com/rjwalters/geode-fem/issues/1185). Amendments 1–6 and the amendment-6 addendum governed the pilot. Amendments 7 (revision 1 plus adoption corrections) and 8 now govern the prospective main study. The frozen spaces remain 8 scalars vs 8+10 offsets, with LENGTH/SD1/ScaleR/OPERATING_ANGLE fixed and physical current 9.6777 A. Targets T50/T90/T99 are set before method outcomes. The pilot budget was 60 measured cold validation-forward wall-time units; main runs receive 120 with a 60-unit checkpoint. All in-run method costs are charged. Shared reference calibration is recorded separately, and the final comparison also reports end-to-end costs.
- Remesh/torque guards and validation infrastructure (#1190/#1192/#1194) are merged; the optimizer harness #1196 merged at `89e6c4e127a5fbef7e4b2e0f10b90e70bd1193b4`. The 36-run development-host pilot merged in PR #1200 at `43db1c626bdc2367f896905d7807979789ee22b3`, from reviewed head `66de22d4be1563a90e0dbe9a7a74caeab406dab8`. It recorded 13 failures (11 unbuildable slot flanks, two fresh meshes below the 5-degree minimum-angle policy), 13 budget stops and 10 T99 stops. Space-B T99 attainment is 6/6 SLSQP, 4/6 COBYQA and 0/6 empirical RBF surrogate; space A has no T99 attainment. Counts include earlier target crossings in runs that subsequently failed; final run status remains separate. These loaded-host observations are feasibility/budget evidence only. Amendments 7 and 8 are adopted for the main study; targets remain frozen, and main runs still require reviewed implementation and an idle-host session. No smoke result is paper evidence. The empirical cubic-RBF surrogate must be named as such, not a reduced physical model. Its half-current prediction is at a training current. Post-run .25/.75 diagnostics evaluate a frozen linear interpolation between current-specific RBF predictions; their separately recorded costs do not affect the stopped run or attainment times. These test the added current interpolation, not intrinsic current generalization.

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

A [pilot-only figure preview](refs/pilot-figure-preview/README.md) separates design starts and marks failures, budget stops and T99 attainment. Its renderer checks all 36 raw traces at the #1200 merge commit and records every input hash. This prepares a readable comparison layout; it does not enter v6 or turn loaded-host pilot timing into main-study evidence. The main figure must include the adopted L0 methods, geometric admissibility and 60/120-FE checkpoints using the actual main records.

## Publication-readiness goal (operator, 2026-10-18)

The operator asked Codex to keep working with Claude until paper 3 is as ready to publish as the first two. This retains the stronger research objective already in the brief; it does not turn the passing v6 audit into evidence of optimization speedup. The first two portfolio entries are themselves revised drafts with older audited versions, so their current status is context rather than a reason to lower the motor paper's evidence requirements.

Completion requires the following evidence, checked at the final manuscript's actual commit:

| Requirement | Evidence needed | Current state / owner |
|---|---|---|
| Reproduced nonlinear motor and verified derivatives | Pinned raw arrays, independent verification and accurate scoped claims | V6 audit PASS at `1dd8cdef`; archived verbatim in `.6.audit/` |
| Valid optimization domain and trustworthy solver callbacks | Adopted amendment, complete guard inventory, unit-consistent margins, solver-visible rejection/restoration, charged mesh repair, independent implementation review | Amendment 7 adopted with admissible-anchor and target-credit corrections; implementation #1201 in progress |
| Matched useful-design-space comparison and cost | Frozen starts/targets/cost rules; complete idle-host main records, paired results, failed/censored outcomes, 60-FE checkpoint if budget expands | Pilot complete in #1200; no main-run results |
| Inexpensive physical competitor | Named physical model, calibration and adaptation protocol frozen before its outcomes, all training/setup costs charged, same final target | L0 nonlinear FEM screened locally; amendment 8 adopted for COBYQA and SLSQP on L0; Codex implementation #1202 follows #1201; no optimizer results |
| Candidate reliability | Common validation plus L3/g40/interface and withheld-condition checks at the reported new candidates; limits on numerical error | Original Opt120 checks exist; new candidate checks still needed |
| Final manuscript and figures | New immutable version integrating measured results without overstating generality, corrected v6 N1, reproducible figures and claim guards | V6 is preserved; next substantive version pending |
| Independent review and audit | Fresh reviews, resolved critical findings, citation/numerical/artifact audit on exact final content | V6 passed for its existing scope; future results need new checks |
| Submission-ready artifact | Author/venue metadata resolved, PDF and source package verified from clean checkout, public provenance/availability links, selected target formatting | Author Robb Walters; venue open; final package pending. Actual submission/publication is not implied by this goal. |

There is no requirement to manufacture a favorable speedup: a null or negative result must be retained. Publication claims must match whichever comparison the data support, including the limits of five design starts and repeated timing trials.

### Original-plan reconciliation (reviewed, 2026-10-18)

The initial brief and October 16 screen contain broader proposed deliverables than the adopted main-run protocol. The table retains those distinctions; Squad review #18 records the scientific scope decisions below. These decisions do not confer completed experimental evidence.

| Original proposed deliverable | Adopted main-run evidence | Remaining distinction |
|---|---|---|
| CMA-ES as well as trust-region derivative-free search | COBYQA, empirical RBF, and both COBYQA/SLSQP on coarse nonlinear FEM | CMA-ES is not in the 100-run matrix; no superiority claim over it or over all credible alternatives is supported |
| Mean-torque floors at each loaded current | A full-current mean floor; ripple at 0, 0.5 and 1; post-run .25/.75 checks | A full-current floor is not a half-current mean constraint |
| Minimum gap/bridge thickness, bounded magnet area and fixed envelope/length | Fixed envelope/length/control policy, published variable bounds, construction and mesh guards, prospective 5% normalized construction margins | Construction feasibility is not a manufacturing constraint; equal parameter boxes do not hold magnet volume constant; any claimed resource or feature limits need explicit numerical definitions and checks |
| Elapsed time, CPU-hours and peak memory | Measured wall-time ledger, solve counts, fixed process/thread limits; CPU deltas and scoped RSS observations required in #1202 | Resource instrumentation is pending; process-wall hours are not CPU-hours and separate process high-water marks are not a simultaneous process-tree peak |
| Cheap-model response changes and scaled torque-residual Jacobian singular values across perturbed designs | Two local scalar L0/L1 gradient comparisons; prospective paired low/high-fidelity outputs | Gradient-vector cosine is not an effective-dimension analysis; no claim of intrinsic high-dimensional necessity follows |

The original screen labels these as a proposed model and execution plan. A revised paper may make a narrower empirical claim if that change is explicitly justified and reviewed, but it must retain the user's central test: useful, refinement-validated optimization and honest end-to-end cost against a credible inexpensive physical competitor. Scope reconciliation must not be used to turn unmeasured performance into a completed result.

Claude independently verified the [geometry-bounds derivation](refs/geometry-bounds-audit.md) in review #18. The [verbatim review](refs/geometry-bounds-review.md), SHA-256 `14346ff47cd567796d9e7ca4adf11bf4248fcdf18cfa1e6d4a9932c9f7387bb1`, checks six premises against pinned source and confirms all three conclusions: magnet area in [20,300] mm² (initially 133), continuous rotor-to-bore separation greater than 0.32876 mm, and an outer surface-to-pocket/magnet web of at least 0.5 mm. The review also derives tighter space-specific bounds. This discharges the corresponding physical-bound proposal without changing the domain. It does not establish fixed magnet volume, every-feature bounds, mesh-approximation guarantees or manufacturing qualification.

The remaining decisions from #18 are:

- CMA-ES and other population/evolutionary methods are explicitly deferred. The paper compares the tested model-based derivative-free method, empirical surrogate and gradient methods; it claims no superiority to untested optimizers or an entire algorithm class.
- Only full-current mean torque is constrained. Report every final candidate's half-current mean against theta0 as a diagnostic; do not claim a partial-load torque guarantee. The .25/.75 checks remain separate.
- Add measured user/system CPU time, including children with explicit measurement boundaries, and RSS observations to #1202, alongside wall time. The budget remains measured wall time. Resource measurements must distinguish cumulative process high-water RSS from per-request changes or a simultaneous process-tree peak; do not sum RSS high-water marks into a claimed peak. Record platform units and unavailable metrics explicitly.
- Defer the residual-Jacobian effective-dimension study. Report observed differences between spaces A and B without calling them intrinsic dimensional necessity.

No targets, starts, budgets or optimizer trajectories are changed by this scope decision. The central useful-design-space, refinement-validity and inexpensive-physics comparison remains required.

### Main-result figure guidance

Claude reviewed the pilot preview in Squad message 604. For the five-method main plot, color should identify optimizer and line style should identify spatial resolution (L1 solid, L0 dashed). Show timing repeat 2 as a thinner line with the same encoding, or in a supplement; it is not an independent design replicate. Extend the axis to 120 FE with a vertical 60-FE checkpoint. Include a per-start target-attainment table showing first T50/T90/T99 cost and explicit censored cells. Generate that table from the same verified traces as the curves. The pilot preview stays unchanged as a pilot-only artifact.

## Amendment 7 review evidence (2026-10-18)

Proposed text: [issue comment 6247054252](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247054252). Codex returned **changes required** in Squad review #15. The decisive issue is that an optimizer requesting f(x) must receive the value and derivative at x, never at a silently shortened point y. The same coordinate must identify the objective, constraints, gradients, cache and validation record. See SciPy's [objective/Jacobian contract](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.minimize.html).

The reproducible [domain probe](refs/amendment7-domain-probe.py) and [recorded output](refs/amendment7-domain-probe.json) use the pilot lock (SciPy 1.18.1, NumPy 2.5.4), a two-dimensional unit-circle constraint, and the globally defined analytic objective −x−y. SLSQP requests seven nonlinear-infeasible objective evaluations out of nine; COBYQA requests twenty out of twenty-four. Thus adding nonlinear constraints does not prevent an invalid FEM request. This is a contract counterexample, not a motor experiment or timing comparison.

A separate diagnostic returns NaN only as a domain-rejection signal to COBYQA. The pinned implementation maps it to its internal extreme barrier (`scipy/_external/cobyqa/problem.py`). On this toy, it returns success at (1,0), with objective −1 rather than the known −sqrt(2) optimum. This does not show that COBYQA fails generally; it shows why a nominal success flag or the presence of a rejection mechanism does not establish reliable handling of domain boundaries. A real integration needs tests of visited coordinates, failure semantics, initialization and recovery, not just exit status.

Required protocol clarifications before main runs:

- SLSQP restoration must be visible to the solver: terminate the current invocation and explicitly restart at the restored point, recording state reset and costs, or validate a true domain-aware solver implementation. Do not silently project inside callbacks. Freeze the exact COBYQA rejection and surrogate initial-set handling too.
- Define raw margins m_i, fixed positive scales s_i with matching units, c_i=m_i/s_i, and residual c_i−0.05. Handle unsafe upstream expressions and discrete guards explicitly. The arc discriminant has squared-length units. Zero reference margins need guard-specific treatment. A 5% reference margin is a prospective design-domain choice, not a proven universal numerical safety threshold.
- Separate Opt120's historical construction regression from eligibility in the new fixed-size/control problem. It need not satisfy a domain it was not optimized in.
- Preserve already valid frozen starts; document deterministic replacements if new constraints invalidate them. Freeze seeds 3/4 and matched scalar coordinates before outcomes. Five starts support a bounded paired benchmark; two timing repeats are not two independent design discoveries.
- Freeze mesh-repair variants, order, total attempt cap and failure categories. Candidate-dependent mesh failure is numerical feasibility, not necessarily infrastructure failure. Retain 5 degrees unless a separate evidence-backed amendment changes it.
- A 120-FE budget with the original 60-FE checkpoint is reasonable if applied uniformly and declared after the pilot, before main runs. Only validations finished by each checkpoint count. Preserve targets, failed/censored records and separate pilot/main trajectories.

Revision 1 ([comment 6247097203](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247097203)) was accepted for implementation in Squad review #16, with an explicit correction: restoration anchors must be admissible, not merely buildable; mesh restoration also needs an anchor that passed the mesh policy. The implementation must freeze the remaining guard scales and repair settings and pass independent review. Target credit must separately require geometric admissibility, since optimizer callbacks can expose constraint-infeasible iterates. Protocol acceptance does not approve an implementation or a performance claim.

The amendment was subsequently [adopted with both corrections](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247152017). Implementation is tracked by [#1201](https://github.com/rjwalters/geode-fem/issues/1201). Main runs await the reviewed implementation, method integration, frozen starts and host selection.

## Inexpensive physics screen and amendment 8 proposal

The [screen script](refs/coarse-physics-screen.py) reconstructs the ripple-squared objective and eight free scalar gradient components from the hash-verified v6 raw records. Its [output](refs/coarse-physics-screen.json) includes input hashes, full vectors and limitations. In unit-box coordinates:

| Stored design | L0 vs L1 objective difference | Objective-gradient cosine | Relative gradient-vector difference |
|---|---:|---:|---:|
| Initial | +0.823% | 0.999566 | 3.168% |
| Perturbed | −0.281% | 0.999841 | 1.991% |

These are two local comparisons using ten angles and the prior fixed unscaled current policy. The perturbed design is not a prospective main-run start. They do not test the grouped-control directions, constrained descent, or optimizer paths. Small gradient components can disagree despite a high cosine. Existing cost records use a different, loaded-host workload and establish no optimization speedup. L2 records contain only full current and cannot supply the same aggregate comparison.

This motivates a coarse nonlinear FEM competitor: it retains the nonlinear material law and solves the field equations at lower spatial resolution. It is not a magnetic-equivalent circuit or a reduced-basis model. The proposed [amendment 8](refs/amendment8-physical-baseline.md) pairs L0 with both derivative-free COBYQA and safeguarded SLSQP, holding validation and cost rules fixed. Actual validated optimization outcomes, including failures, must determine the conclusion. The screen does not discharge the open physical-baseline requirement.

Claude independently accepted that exact proposal at `ca25a11bda465add9240ac794ed787a081d50772` in Squad review #17 and [adopted it on #1185](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247152174). Keep the linked proposal unchanged as the reviewed text. The comparison has 100 runs: five methods, two spaces, five starts and two timing repeats. A/B/D/E are the optimizer-by-resolution factorial comparison; C is the empirical RBF competitor. Codex owns the additional L0 method integration in [#1202](https://github.com/rjwalters/geode-fem/issues/1202), after #1201 merges. Its acceptance criteria bind a single validation-cost denominator, actual mixed-resolution wiring, complete diagnostics and the five-method scheduling rules.

For planning only, 100 runs spending their full 120-unit budgets at approximately 35 seconds per unit require about 117 aggregate process-hours, or 19.4 hours at six continuously occupied workers, excluding setup and post-run checks. That unit is a proxy from the pilot's single-process diagnostic on a loaded host, not an idle-host measurement. Early stops may shorten the runs; setup, diagnostics and contamination reruns may lengthen the session. The actual main-run unit must be measured on the selected host. Host preference has been requested while implementation proceeds.
