# Experiments requested for the next motor-paper iteration

Requested 2026-10-17 after v2's evidence audit. Coordinate execution in Squad with Claude; #1179/#1180 already own grouped geometry and derivative validation. Do not duplicate their runs. These are proposed measurements, not completed results or preselected favorable outcomes.

## Current status (v18, 2026-10-22)

All requested optimization experiments have run; the sections below are the historical request and launch record.

- **Main study** (#1185, preregistered): 100 runs, five methods, two spaces, complete; 213 final checks. SLSQP with adjoints beat COBYQA in all 45 of the 50 paired comparisons where both succeeded (3.10 and 7.79 at the primary targets).
- **Ablation FD-cold** (F in the records; #1218, post hoc, protocol frozen before any run): SLSQP with cold-started forward-difference stencils, 40 runs. FD-cold/A′ = 2.47 and 4.11.
- **Ablation FD-warm** (F_w in the records; #1224, post hoc, protocol frozen before any run): warm-started stencils, 20 runs (repeat 1 only, as the protocol fixed). FD-warm/A′ = 1.71 and 2.70, the paper's headline. V18 decomposes B/A into a gradient factor (1.71–2.47; 2.68–4.10) and an optimizer factor (1.22–1.76; 1.91–2.92) from these records; no new run. Warm stencils are less accurate on small components at the frozen Newton tolerance.
- **Archive:** all three sessions and their final-check results in `motor-paper-raw-traces.tar.zst` (2,790,582,024 bytes, SHA-256 `1dfbf388…a1b98b07`; rebuilt for v17, unchanged since), deposited at DOI 10.5281/zenodo.23580550 (cited in v21's data statement).
- **Untested follow-ons:** CMA-ES and population methods; a reduced physical model (magnetic equivalent circuit); direct or semi-analytic sensitivities.

## Evidence and execution status as of v6 (historical)

- Experiment 1 merged as #1182 at `224332c526be1191183af82b610973c7778de720`. V6 retains fresh export/NURBS L2–L3 records, with coupling torque for harmonic fields and outer-band Arkkio only for the checked fresh conforming meshes. Fixed unscaled and physical current are distinct at Opt120's radial scale.
- Experiment 3 merged as #1191 at `8acd21ecf85d89fe077275a3c04ca70b53963d43`. The coarse rerun reproduces all 60 saved torques before refinement; 24×24 brings cogging std agreement to 0.10%. This resolves the coarse-reference explanation at matched order, without claiming a continuum bound.
- Experiment 2's authoritative preregistration is [#1185](https://github.com/rjwalters/geode-fem/issues/1185). Amendments 1–6 and the amendment-6 addendum governed the pilot. Amendments 7 (revision 1 plus adoption corrections) and 8 govern the main study. The frozen spaces remain 8 scalars vs 8+10 offsets, with LENGTH/SD1/ScaleR/OPERATING_ANGLE fixed and physical current 9.6777 A. Targets T50/T90/T99 are set before method outcomes. The pilot budget was 60 measured cold validation-forward wall-time units; main runs receive 120 with a 60-unit checkpoint. All in-run method costs are charged. Shared reference calibration is recorded separately, and the final comparison also reports end-to-end costs.
- Remesh/torque guards and validation infrastructure (#1190/#1192/#1194) are merged; the optimizer harness #1196 merged at `89e6c4e127a5fbef7e4b2e0f10b90e70bd1193b4`. The 36-run development-host pilot merged in PR #1200 at `43db1c626bdc2367f896905d7807979789ee22b3`, from reviewed head `66de22d4be1563a90e0dbe9a7a74caeab406dab8`. It recorded 13 failures (11 unbuildable slot flanks, two fresh meshes below the 5-degree minimum-angle policy), 13 budget stops and 10 T99 stops. Space-B T99 attainment is 6/6 SLSQP, 4/6 COBYQA and 0/6 empirical RBF surrogate; space A has no T99 attainment. Counts include earlier target crossings in runs that subsequently failed; final run status remains separate. These loaded-host observations are feasibility/budget evidence only. Amendments 7 and 8 are adopted for the main study; targets remain frozen. Reviewed main implementation merged through PR #1206, and the main session has started on a dedicated host (dated status below). No smoke result is paper evidence. The empirical cubic-RBF surrogate must be named as such, not a reduced physical model. Its half-current prediction is at a training current. Post-run .25/.75 diagnostics evaluate a frozen linear interpolation between current-specific RBF predictions; their separately recorded costs do not affect the stopped run or attainment times. These test the added current interpolation, not intrinsic current generalization.

The original experiment requests below retain the wider research scope. Where they conflict with the frozen pilot, the issue's amendments govern that pilot. The strongest inexpensive-physics comparison and broader geometric feasibility constraints still need explicit evidence before the full research hypothesis can be claimed.

## Main launch and remaining evidence (observed 2026-10-18, 19:32 UTC)

[PR #1206](https://github.com/rjwalters/geode-fem/pull/1206) merged at
17:53:12 UTC, with merge commit
`9486a9da58beeca91968d4d3608f06b998abff45`. The main session
`main-20261018T181644Z` runs from that source. Its retained setup records
show anchor preflight `PASS`, a clean source tree, bitwise agreement between
the two spaces at the anchor, and both calibrations accepted for dispatch.
The common measured unit is 70.935572288 seconds; N = 15, the run budget is
120 FE and the checkpoint is 60 FE. These are setup facts, not optimization
results. A read-only host observation at 19:32:29 UTC confirmed the session
and evaluator processes were still running. No interim objective values,
target attainment or method comparisons are used in this status update.

The setup records are retained locally in
`/Volumes/Stripe/scratch-codex-review/exp2-main-session/setup-records/`.
They must enter the verified closed-session artifact before the final paper
relies on them; this local locator is not a public availability link.
Identity checks for that handoff are:

| Record | SHA-256 |
|---|---|
| `session.json` | `451427f45bbf8ea108a44a3c1de006b6e243b79f25cf2971787ab97f0da8d6d5` |
| `anchor_preflight.json` | `bde4e260e549a134e6632333439fb18c8ebe7d25b6dcf541d5b6715cb0ea2628` |
| `calibration_primary.json` | `1496a44cbf5157b506fdaee20117df2682cd4ba5bce96502000531e83bde36f2` |
| `calibration_secondary.json` | `f92575e2268c99be8981b4d9edee41bcb0675d859ba81cc47fbb4db1af017d55` |
| `bindings.json` | `4dac96a80c22c6ff35981124afdbd268b8049231682e6ab1d3da32cb66815d89` |

The first launch was refused before preflight or FEM because the remesher's
package environment had not been prepared. The environment was prepared
with the evaluator's own command and verified offline before the current
session launched. The refused attempt is preserved separately; it is not
an optimizer run. Host operations and that refusal are recorded in the
session's local `operations-handoff.json` for the eventual artifact package.

[PR #1207](https://github.com/rjwalters/geode-fem/pull/1207), currently at head
`19cc493ed9992885cefcc3192657255ef1eeddc2`, implements the closed-capture,
selection, final-check and reporting chain and awaits independent review.
Its [evidence index](https://github.com/rjwalters/geode-fem/blob/19cc493ed9992885cefcc3192657255ef1eeddc2/benchmarks/motor_optimization/final_refinement/EVIDENCE.md)
distinguishes synthetic cohort tests from bounded native execution evidence.
Neither is a complete main-cohort result. The remaining evidence sequence is:

1. Close and verify the full main session, including every attempt, exclusion,
   recovery and resource record; retain the frozen starts, queue and costs.
2. Complete independent pipeline review and execute the frozen final cohort
   with matched reference settings, explicit failed/unavailable checks and
   separate post-run costs.
3. Generate paired per-start curves, target-attainment tables, refinement
   comparisons and resource summaries from those verified records. Preserve
   censoring and the distinction between timing repeats and design starts.
4. Integrate the results and actual execution metadata into a new immutable
   manuscript, correct v6's deferred caption finding, then complete fresh
   independent reviews, numerical/citation audit and rendered-figure checks.
5. Verify the PDF and public source/data package from a clean checkout.

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

A [prospective methods fragment](refs/main-study-methods-draft.tex) prepares the next manuscript from the adopted protocol. A [clause-by-clause coverage audit](refs/main-study-methods-protocol-audit.md) records missing or implicit details against the adopted amendments and preserves the exact source snapshot. The [author response](refs/main-study-methods-coverage-followup.md) records the subsequent prospective-text revision and the evidence still pending. A [prospective settings supplement](refs/main-study-settings-supplement.tex), with [source provenance](refs/main-study-settings-supplement.json), supplies parameter/group tables and the adopted algorithm while distinguishing unreviewed implementation observations. The [space-motion and finite-cap clarification](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6248418735) is recorded before main outcomes: cross-space differences do not isolate added offsets, and finite cap hits must be reported. It explicitly contains no main-study result and retains a pending execution-metadata marker. It must be reconciled with the merged implementation and actual manifests, then reviewed with the new manuscript; it is not part of the audited v6 text.

The [prospective main-run runbook](refs/main-run-runbook.md) covers host preflight, calibration and scheduling, contamination handling, interruption recovery, evidence archiving and the #1203 handoff. For the actual launch, its command templates were bound to the reviewed source at `9486a9da58beeca91968d4d3608f06b998abff45`. Durable session recovery and clean-calibration sequencing merged through PR #1206. The prospective preparation snapshot remains a historical record, not the actual launch receipt. Recovery replaces eligible whole attempts; it is not optimizer-state resume.

## Publication-readiness goal (operator, 2026-10-18)

The operator asked Codex to keep working with Claude until paper 3 is as ready to publish as the first two. This retains the stronger research objective already in the brief; it does not turn the passing v6 audit into evidence of optimization speedup. The first two portfolio entries are themselves revised drafts with older audited versions, so their current status is context rather than a reason to lower the motor paper's evidence requirements.

Completion requires the following evidence, checked at the final manuscript's actual commit:

| Requirement | Evidence needed | Current state / owner |
|---|---|---|
| Reproduced nonlinear motor and verified derivatives | Pinned raw arrays, independent verification and accurate scoped claims | V6 audit PASS at `1dd8cdef`; archived verbatim in `.6.audit/` |
| Valid optimization domain and trustworthy solver callbacks | Adopted amendment, complete guard inventory, unit-consistent margins, solver-visible rejection/restoration, charged mesh repair, independent implementation review | Amendment 7 and #1202 integration reviewed and merged through #1206 at `9486a9da`; main source fixed to that commit |
| Matched useful-design-space comparison and cost | Frozen starts/targets/cost rules; complete idle-host main records, paired results, failed/censored outcomes, 60-FE checkpoint if budget expands | Pilot complete in #1200; main session running; closed cohort and paired outcome analysis pending |
| Inexpensive physical competitor | Named physical model, calibration and adaptation protocol frozen before its outcomes, all training/setup costs charged, same final target | L0 COBYQA/SLSQP implementation merged through #1206 and included in the live main matrix; complete optimizer outcomes and comparison pending |
| Candidate reliability | Common validation plus L3/g40/shifted-grid/tight-Newton/interface and withheld-condition checks at the reported new candidates; limits on numerical error | Original Opt120 checks exist; frozen selection and final-check pipeline implemented in PR #1207, under independent review; actual main-cohort checks pending |
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
| Elapsed time, CPU-hours and peak memory | Measured wall-time ledger, solve counts, fixed process/thread limits; CPU deltas and scoped RSS observations required in #1202 | Resource instrumentation merged through #1206; complete main-cohort measurements remain pending. Process-wall hours are not CPU-hours and separate process high-water marks are not a simultaneous process-tree peak |
| Cheap-model response changes and scaled torque-residual Jacobian singular values across perturbed designs | Two local scalar L0/L1 gradient comparisons; prospective paired low/high-fidelity outputs | Gradient-vector cosine is not an effective-dimension analysis; no claim of intrinsic high-dimensional necessity follows |

The original screen labels these as a proposed model and execution plan. A revised paper may make a narrower empirical claim if that change is explicitly justified and reviewed, but it must retain the user's central test: useful, refinement-validated optimization and honest end-to-end cost against a credible inexpensive physical competitor. Scope reconciliation must not be used to turn unmeasured performance into a completed result.

Claude independently verified the [geometry-bounds derivation](refs/geometry-bounds-audit.md) in review #18. The [verbatim review](refs/geometry-bounds-review.md), SHA-256 `14346ff47cd567796d9e7ca4adf11bf4248fcdf18cfa1e6d4a9932c9f7387bb1`, checks six premises against pinned source and confirms all three conclusions: magnet area in [20,300] mm² (initially 133), continuous rotor-to-bore separation greater than 0.32876 mm, and an outer surface-to-pocket/magnet web of at least 0.5 mm. The review also derives tighter space-specific bounds. This discharges the corresponding physical-bound proposal without changing the domain. It does not establish fixed magnet volume, every-feature bounds, mesh-approximation guarantees or manufacturing qualification.

The remaining decisions from #18 are:

- CMA-ES and other population/evolutionary methods are explicitly deferred. The paper compares the tested model-based derivative-free method, empirical surrogate and gradient methods; it claims no superiority to untested optimizers or an entire algorithm class.
- Only full-current mean torque is constrained. Report every final candidate's half-current mean against theta0 as a diagnostic; do not claim a partial-load torque guarantee. The .25/.75 checks remain separate.
- Add measured user/system CPU time, including children with explicit measurement boundaries, and RSS observations to #1202, alongside wall time. The budget remains measured wall time. Resource measurements must distinguish cumulative process high-water RSS from per-request changes or a simultaneous process-tree peak; do not sum RSS high-water marks into a claimed peak. Record platform units and unavailable metrics explicitly.
- Defer the residual-Jacobian effective-dimension study. Report observed differences between spaces A and B without calling them intrinsic dimensional necessity.

No targets, starts, budgets or optimizer trajectories are changed by this scope decision. The central useful-design-space, refinement-validity and inexpensive-physics comparison remains required.

### Start sampling (explicit rule before main outcomes)

Squad review #21 and the [start-eligibility clarification](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247585770) retain the moved-mesh preflight: a start must be buildable and geometrically admissible in both spaces, and the theta0 mesh moved to it must pass the mesh policy **without remeshing**, at both L0 and L1. The optimizer's later iterates may use the full four-attempt repair policy. Try the frozen pilot indices first and keep them if eligible; record every rejection and deterministic replacement. Geometry/mesh preflight costs are recorded outside the run budgets; no FEM is needed for this selection.

This samples the part of the domain reachable by acceptable motion of the reference meshes, tending toward moderate deformations. The five starts are not uniform draws from the admissible box, and two timing repeats are not additional independent designs. Report the comparison as a paired per-start case study under this sampling rule. The historical pilot checked L0; the main study adds L1 and the prospective normalized construction margins.

### Final-refinement selection (reviewed before main outcomes)

Squad review #19, the [adopted amendment-8 addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247383821), and its [feasibility/repeat clarification](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247421487) define the final check cohort. [Implementation #1203](https://github.com/rjwalters/geode-fem/issues/1203) follows #1202; the following are prospective checks, not completed evidence.

- For each method and space, select the lowest common-validated feasible objective among eligible uncontaminated main runs. Break ties by start index, repeat index, then validation ordinal. Earlier eligible validations from a subsequently failed run remain eligible. No feasible representative is an explicit outcome. Retain every considered record and its selection/rejection reason; never replace a selected design after its refinement fails.
- For each method, space and target, additionally select the earliest attaining validation across starts using repeat-1 timing: at most 30 crossing designs. Deduplicate identical geometry/settings against the ten possible representatives with complete provenance. Use repeat-1 records only, with absent crossings recorded rather than replaced by repeat 2.
- Geometry/mesh admissibility is additional to the existing full-current mean floor and completed-within-budget validation requirements. Target crossings also retain the registered objective/error-margin test. The adopted clarification explicitly preserves all these acceptance rules.
- Check theta0 and selected designs at L3/h93/g20 and L2/h93/g40, at the three training currents, retaining half-current and .25/.75 diagnostics. Conforming outer-band Arkkio checks require zero stator offsets and fresh meshes under the existing supported-rotation policy. Nonzero stator offsets must report `no conforming check (model-dependent)`.
- The [adopted part-2 addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6248492318) (Squad review #25) adds two checks for theta0 and the same deduplicated cohort at all three training currents: L2/h93/g20 with Newton `rel_tol=1e-12` versus default `1e-8`, and an independently solved L2/h93 grid at 0.5, 1.5, ..., 19.5 degrees with default tolerance. Each current sweep starts cold, then continues between angles. Retain signed waveform/mean/std/F changes and observed failures, comparing candidates with theta0 at matching settings. Even/odd g40 subsets are optional sampling diagnostics only; their continuation history does not constitute an independent shifted solve. No candidate replacement or target-credit changes are permitted. Implementation is in PR #1207 and awaits independent review; actual main-cohort execution and measured costs remain pending.
- Keep all post-run costs separate. Time-to-target remains measured at common L2/h93/g20 validation; these best-observed representatives and earliest crossings do not certify every crossing or typical-start behavior. Report failed checks and observed refinement changes alongside the affected claims.

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

The amendment was subsequently [adopted with both corrections](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247152017). Implementation is tracked by [#1201](https://github.com/rjwalters/geode-fem/issues/1201). Its reviewed implementation merged through #1204 at `5c998bda`. Reviewed #1202 integration merged through PR #1206; the main session has passed setup and is running at cloud concurrency N = 15.

## Inexpensive physics screen and amendment 8 proposal

The [screen script](refs/coarse-physics-screen.py) reconstructs the ripple-squared objective and eight free scalar gradient components from the hash-verified v6 raw records. Its [output](refs/coarse-physics-screen.json) includes input hashes, full vectors and limitations. In unit-box coordinates:

| Stored design | L0 vs L1 objective difference | Objective-gradient cosine | Relative gradient-vector difference |
|---|---:|---:|---:|
| Initial | +0.823% | 0.999566 | 3.168% |
| Perturbed | −0.281% | 0.999841 | 1.991% |

These are two local comparisons using ten angles and the prior fixed unscaled current policy. The perturbed design is not a prospective main-run start. They do not test the grouped-control directions, constrained descent, or optimizer paths. Small gradient components can disagree despite a high cosine. Existing cost records use a different, loaded-host workload and establish no optimization speedup. L2 records contain only full current and cannot supply the same aggregate comparison.

This motivates a coarse nonlinear FEM competitor: it retains the nonlinear material law and solves the field equations at lower spatial resolution. It is not a magnetic-equivalent circuit or a reduced-basis model. The proposed [amendment 8](refs/amendment8-physical-baseline.md) pairs L0 with both derivative-free COBYQA and safeguarded SLSQP, holding validation and cost rules fixed. Actual validated optimization outcomes, including failures, must determine the conclusion. The screen does not discharge the open physical-baseline requirement.

Claude independently accepted that exact proposal at `ca25a11bda465add9240ac794ed787a081d50772` in Squad review #17 and [adopted it on #1185](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247152174). Keep the linked proposal unchanged as the reviewed text. The comparison has 100 runs: five methods, two spaces, five starts and two timing repeats. A/B/D/E are the optimizer-by-resolution factorial comparison; C is the empirical RBF competitor. The additional L0 method integration in [#1202](https://github.com/rjwalters/geode-fem/issues/1202) merged through PR #1206 after #1201. Its acceptance criteria bind a single validation-cost denominator, actual mixed-resolution wiring, complete diagnostics and the five-method scheduling rules.

For planning only, 100 runs spending their full 120-unit budgets at approximately 35 seconds per unit require about 117 aggregate process-hours, or 19.4 hours at six continuously occupied workers, excluding setup and post-run checks. That unit is a proxy from the pilot's single-process diagnostic on a loaded host, not an idle-host measurement. Early stops may shorten the runs; setup, diagnostics and contamination reruns may lengthen the session. The actual main-run unit must be measured on the selected host. This historical planning proxy is superseded by the cloud probe described in the runbook; its timings still do not replace the final main calibration.

## Pre-main repair, admissibility and interruption rulings (2026-10-18)

[Comment 6248970362](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6248970362)
replaces the ineffective 2D Optimize/Netgen repair with algorithm 5 (Delaunay),
or algorithm 1 when the default is already 5. Attempts 3/4 remain noncumulative
size factors 0.9/0.8; actual algorithm and distinct-mesh fixture evidence are
required. Target credit and best-feasible selection allow every scaled
h_i >= -1e-12; optimizer constraints and exact buildability guards are unchanged.
The paper must disclose the implicit geometric exclusion from the 5-degree
mesh floor and the unguarded space-B slot-opening wall (down to 0.16 micrometres
at minimum SW3 / maximum CPS1). These adopted rules are implemented in the reviewed PR #1206 source;
that implementation is not performance evidence.

[Comment 6249073475](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6249073475)
adopts the interrupted-session policy from Squad review 28. It preserves the
first clean completed result, allows at most one verified external-interruption
replacement per job, keeps scientific failures, and repairs post-run diagnostics
without rerunning optimization. Recovered jobs follow the remaining registered
queue in original relative order; source/environment/inputs/host/N and the
original recorded unit must match. No recalibration or reset of contamination
allowances is permitted. Report every replacement, reason, evidence and original
partial cost, and count affected jobs. Unverified interruptions remain unresolved.
The runbook describes the adopted policy; its implementation in #1202
merged through the independently reviewed PR #1206. After the dependency merge,
verification at `bc1527e8` passed 98 safeguard mutations, 20 frozen-start checks,
six admissible central and two inward-boundary gradient checks, and both-space
L0-training/L2-validation smokes. Earlier records remain historical. These checks
establish setup/numerical wiring, not optimizer performance.


## Cross-platform anchor recomputation (adopted 2026-10-18)

[Comment 6252347196](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6252347196)
separates strict anchor-file parsing from numerical recomputation on another host.
Before calibration, the host must reproduce F0 and full-current mean within
`1e-9` relative, and every torque sample at every current within `1e-9 N m`
absolute. Mesh, physics and reference-data hashes stay exact; same-host
determinism and space-A/space-B bitwise checks remain. Newton counts are
recorded without requiring equality. The committed anchor always supplies
targets, delta-F and the mean floor. The probe's earlier `1e-6` stop threshold
is not the adopted main acceptance rule.

The cloud probe has no optimizer runs. Its retained data reproduce the Mac
anchor to about `2.4e-13` relative in F0 and `7.4e-15` in mean, with maximum
torque difference `1.1e-13 N m`. The runbook records the raw archive, arithmetic
audit scope, actual interpreter/backend and N = 15 selection. The reviewed PR #1206 implementation adds an automatically recorded
pre-calibration waveform gate. The actual main session passed that gate and
both calibration acceptance checks (dated launch status above); this does not
establish completed optimizer or final-refinement evidence. No CPython
patch version is pinned across platforms; the actual interpreter and numerical
packages are frozen within the session and its recovery generations.
