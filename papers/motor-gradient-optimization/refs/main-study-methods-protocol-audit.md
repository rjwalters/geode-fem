# Prospective Methods: protocol coverage audit

2026-10-18. Requested by Claude in Squad message 711. This is a coverage
review of `main-study-methods-draft.tex` at paper commit
`18316fbf722697dad6990a6a00a28abec6f32ea3`, SHA-256
`d8dd383b78da1471486e5a6e6dcea51b23aa4bf2b47f26df0beb3e260f0c449b`.
It is not a review of completed experiments, an implementation approval, or
an extension of the immutable v6 audit. No main outcomes exist in this draft.

**Finding:** the fragment describes the adopted comparison and most of its
scientific limits, but it is not yet a self-contained reproducible Methods
section. The main gaps are concurrent calibration, exact safeguard/repair
rules and failure categories, equal optimizer options across resolutions,
L0 derivative verification, paired low/high-fidelity records, and explicit
selection eligibility/provenance. These need prose or an explicit reference
to a frozen supplement before incorporation into the next manuscript.

This audit does not require every engineering acceptance test to be printed
in the main article. Where a table below says “supplement,” the final paper
must identify the reviewed artifact and its version; a generic phrase such
as “registered settings” is not sufficient evidence that the detail is
available to its reader. An implementation test and a manuscript description
are separate requirements.

## Sources and precedence

The [source snapshot](main-study-methods-protocol-sources.json) preserves
comment bodies, update timestamps, links and hashes, plus hashes of the
local proposal, draft, EXPERIMENTS and runbook at the reviewed commit.
It excludes the superseded first amendment-7 proposal. These source labels
are used below:

- **A7:** [revision 1, 6247097203](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247097203).
- **A7c:** [adoption corrections, 6247152017](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247152017). These override A7's buildable-only restoration anchor.
- **A8:** [proposal](amendment8-physical-baseline.md), adopted at `ca25a11bda465add9240ac794ed787a081d50772` by [6247152174](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247152174).
- **A8c:** [6247190326](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247190326): factorial A/B/D/E, loaded-host timing proxy, actual guard strictness and discriminant derivative domain.
- **F:** [final-selection addendum, 6247383821](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247383821), with [6247421487](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247421487) overriding its incomplete definition of feasible.
- **S:** [start eligibility, 6247585770](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247585770), with [6247624117](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247624117) correcting the rationale, not the rule.
- **I0:** [original preregistration](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6240051065), only where not superseded (notably the moved-mesh triggers).
- **I1–I6:** inherited amendments 1–6 and their adoption/addenda, individually linked in the inherited-rules table. A7/A8 do not replace these wholesale.
- **R18:** reviewed scope decisions preserved in [EXPERIMENTS](../EXPERIMENTS.md), under “The remaining decisions from #18.” The exact source hash is retained in the snapshot.

A8's proposal says starts pass a “fresh-mesh policy”; S resolves this to
moved-reference-mesh quality **without remeshing** for start selection. The
draft follows S correctly. A8's adoption summary says A–D and calls the
35-second proxy idle-host CPU time; A8c corrects both. Neither error should
be copied into the manuscript. S's initial claim that broader eligibility
necessarily loses pilot pairing was withdrawn; the draft does not repeat it.

Coverage codes: **C** = explicit enough for this clause; **P** = partial or
implicit, with a required clarification; **M** = missing from this fragment.
Draft locations name subsections or recognizable sentences, rather than
line numbers that will move. Rows group closely related clauses; their
count is not a numerical completeness score.

## Amendment 7 and its corrections

| ID / source | Clause | Coverage in the reviewed fragment / action |
|---|---|---|
| 7.0 / A7 introduction | Explain why coupled construction constraints supplement the parameter box; preserve pilot outcomes and fixed targets. | **P.** Geometry constraints and fixed targets appear; the pilot's coupled SW2/SW1/SW4 failure motivation and separation of pilot/main evidence do not. Add a short motivation and identify the unchanged pilot artifact. |
| 7.1a / A7 §1 | Complete smooth/discrete guard inventory, raw margins and units, including upstream-dependent expressions. | **P.** “Construction margins” and discrete checks are named. No inventory or formula table is identified. Cite the frozen guard inventory in a supplement; distinguish smooth constraints from discrete guards. |
| 7.1b / A7 §1, A7c freeze items | Fixed scale equals absolute nonzero theta0 margin; zero-reference scales require individual definitions/justifications; discriminant scales have squared-length units. | **P.** Fixed positive dimension-matched scales are stated, but their actual rule and zero-reference exceptions are absent. Supply the scale table and its provenance. |
| 7.1c / A7 §1, A7c freeze items | Evaluate guards before unsafe sqrt/arcsin; report raw margins and safe downstream values/derivatives rather than downstream NaNs. | **P.** Rejection before a field request is stated, but safe geometry evaluation and downstream derivative handling are not. Describe the rule and cite its checks. |
| 7.1d / A7 §1, A8c | Actual strict/non-strict construction guards define buildability; h=c−0.05 defines admissibility; buildable but inadmissible points are evaluated normally. | **P.** Strictness and both definitions are present. State explicitly that h<0 alone does not trigger restoration or suppress FEM evaluation. |
| 7.1e / A7 §1 | Tau is prospective, never outcome-tuned; record implied physical floors and excluded fixed starts/reference designs. | **P.** The non-certification caveat is explicit. No no-tuning rule, physical-floor table or exclusion record is referenced. |
| 7.1f / A7 §1 | Geometry Jacobians are analytic or geometry-only central FD, checked in CI and charged to geometry_check. | **P.** Geometry costs are mentioned. The derivative source, verification and absence of FEM work in these Jacobians are missing. |
| 7.1g / A7 §1 | Eleven pilot slot-flank failures are non-buildable; theta0 admissible; Opt120 construction remains a historical regression, without relaxing the main domain to admit it. | **M.** Cite the regression evidence and state the historical-design distinction in the supplement. |
| 7.1h / A8c | A zero arc discriminant can satisfy the construction guard while its design derivative is undefined; no invented gradient is allowed. | **P.** Undefined derivatives are generally rejected, but this distinction between value and derivative domains is absent. Specify the case and its regression evidence in the guard supplement. |
| 7.2a / A7 §2 | Never return another point's value/gradient or fabricate values for refused requests. | **P.** Actual restored points and fresh solver state are described. State the requested-coordinate contract explicitly, including objective/gradient/constraint/cache identity. |
| 7.2b / A7 §2, A7c correction 1 | Abort SLSQP; restore from last admissible evaluated point; geometry-only segment bisection; at most 20 probes; restart at actual restored point with fresh state; at most 20 restarts. | **P.** Core anchor, limits and restart behavior are explicit. Geometry-only restoration, exact segment rule and the no-anchor/no-admissible-probe termination are not. Add `restoration_failed` semantics. |
| 7.2c / A7c correction 1 | For mesh-infeasible requests the anchor must have passed mesh policy and restoration must verify mesh quality with costs charged. | **M.** The generic mesh-policy sentence does not specify the restoration predicate. State this special case. |
| 7.2d / A7 §2 | Record rejected x, restored y, restart count and every cost. | **P.** “Every attempt, refusal and restoration” is recorded/charged, but the coordinate and state-reset evidence fields are not identified. Cite the trace schema. |
| 7.2e / A7 §2, A7c | COBYQA uses scipy 1.18.1 NaN extreme barrier; refused point never reaches FEM; record/test initial-set behavior; no success-flag credit. | **P.** NaN barrier and no success-flag credit are present. Exact version and initial-set refusal evidence are absent; “pinned” is not a version. |
| 7.2f / A7 §2 | C never fits refused points; opposite-sign retry, then up to three halvings in each direction; preserve full rank or stop restoration_failed. | **P.** Skip/retries/halvings are named, but count, direction order, full-rank requirement and failure outcome are absent. |
| 7.2g / A7 §2 | C applies buildability checks to geometry and subproblem points; includes h≥0 in its subproblem; geometry participates in feasible-first order. | **P.** Feasible-first geometry constraint is explicit. The subproblem constraint and geometry-point refusal rules need a reference or text. |
| 7.2h / A7 §2, A7c freeze items | Freeze and test each method's refusal/initial-set mechanism before main runs. | **M.** Add the reviewed implementation/test artifact to reproducibility prerequisites. |
| 7.3a / I0, A7 §3, A7c freeze items | Any inverted moved triangle or minimum angle below 5 degrees triggers repair; fresh minimum angle 5 degrees; four attempts: default, Optimize+OptimizeNetgen, size factors 0.9, 0.8; each starts from defaults. | **P.** Four non-cumulative attempts are stated, but triggers, threshold, order and option values are not. Include a compact table or frozen supplement reference. |
| 7.3b / A7 §3 | All-four failure is candidate-dependent mesh infeasibility, handled by each method's refusal rule; real solver/infrastructure errors remain failed. | **M.** Critical failure taxonomy is absent. It affects failure rates and interpretation, not just implementation detail. |
| 7.3c / A7 §3 | Charge all attempts, including failed attempts and mesh restoration. | **C.** The mesh paragraph explicitly charges every attempt/restoration; retain the corresponding trace evidence. |
| 7.4a / A7 §4 | Common 120-unit budget; 60-unit checkpoint uses completed validations only. | **C.** Common-validation subsection states both. |
| 7.4b / A7 §4 | Full curves through 120; never append pilot paths to main runs. | **M.** Add reporting/provenance statement. The budget sentence alone does not specify these. |
| 7.4c / A7 §4 | Keep failures/censored records; no finite speedup from censored target. | **C.** Common-validation and interpretation paragraphs state both. |
| 7.5a / A7 §5, S | Keep eligible pilot indices 1/4; deterministic replacements; seeds 3/4 use first eligible index; paired scalar coordinates. | **P.** Rules are described, but numeric pilot indices and first-eligible replacement semantics are not stated. Cite the frozen start table. |
| 7.5b / A7 §5 | Freeze all five exact starts before any outcome, with indices/reasons. | **P.** Archive language is present; pre-outcome commitment of exact vectors and every rejected index is not explicit. |
| 7.5c / A7 §5 | Paired five-start case study; repeats are timing repeats, not independent designs. | **C.** Starts paragraph makes this distinction. |
| 7.6a / A7 §6 | Pre-run one-minute load ≤N+1; load >1.5N for >60 s contaminates a run; rerun at session end, retain both; host_idle true only when every contaminated run has a clean replacement. | **P.** Gate/monitor/reruns are mentioned. Thresholds, duration, rerun placement and host_idle eligibility are absent. Runbook §2 covers thresholds; add an explicit reference or compact statement. |
| 7.6b / A7 §6, A7c | Record SHA-256 for every training remesh and x for failed/rejected requests; summary in-run range excludes C post-run diagnostic. | **P.** Separate post-run cost is explicit; per-mesh hashes and failed/requested coordinates are absent. Identify the manifest/trace schema. |
| 7.6c / A7 §6, A7c | Reviewed merged implementation and operator-selected host are prerequisites; protocol adoption is no performance evidence. | **P.** Draft status avoids result claims, but execution prerequisites are not listed. Link the runbook and exact merged implementation when available. |
| 7.c2 / A7c correction 2 | Common acceptance additionally requires h≥0, discrete guards and validation-mesh quality, both for attainment and best-feasible selection; inadmissible low-F/high-mean point gets no credit. | **P.** Target-credit geometry checks are named, but final representative eligibility is only called “eligible feasible.” Make the shared conjunction explicit in the selection paragraph too. |

## Amendment 8 and adoption correction

| ID / source | Clause | Coverage / action |
|---|---|---|
| 8.1 / model | L0 nonlinear FEM; same geometry, material, fixed current, h45, g20, three currents, torque estimator and mesh policy; only spatial resolution differs. | **P.** All but the explicit coupling-torque estimator and same-resolution-specific mesh-policy application are stated. Name coupling torque; confirm against the frozen model table. |
| 8.2 / model | Coarse physics is not an empirical fit/MEC/ROM; two-design screen is motivation only; no claim against every cheap model. | **P.** Model distinction and bounded conclusions are present. Identify the screen as non-main, non-performance evidence if it appears with these Methods. |
| 8.3 / methods | D shares B's version/options/radii/scaling/rejection/constraints/callbacks/stopping; E shares A's tolerances/adjoint callbacks/restoration/stopping. | **M.** The table alone does not promise equal optimizer settings across resolutions. Add that rule and a versioned settings table. |
| 8.4 / methods, A8c | A/B and E/D compare optimizers; A/E and B/D compare resolution; A/D changes both; C outside factorial. | **P.** Correct factorial and causal caveat are explicit. Name the matched contrasts to prevent interpretation of A/D alone as a derivative effect. |
| 8.5 / methods | All five methods, two spaces, five starts, two repeats, 100 runs; rotations/reversal; record actual order; no outcome-based omission of E. | **P.** Matrix/order are explicit. State that methods and starts are frozen before outcomes, not selected afterward. |
| 8.6 / calibration | Per-resolution theta0 normalization; model-relative physical mean floors may differ; common validation floor decides credit. | **P.** Normalizations and common floor appear. State explicitly that equal normalized training floors need not be equal physical torques. |
| 8.7 / calibration | No fitted correction, bias correction, cross-level gradient substitution, adaptive refinement or validation-based tuning; later changes require a prospective amendment. | **P.** No fitted objective correction is stated. Remaining non-adaptation rules are absent. |
| 8.8 / calibration | One-time references/meshing/setup reported by resolution; shared and method-specific costs separated; end-to-end totals include both. | **P.** Total/shared/post-run costs appear. Per-resolution and method-specific setup allocation needs a defined accounting table. |
| 8.9 / validation | Common L2/h93/g20, exact anchor hash, targets/margin/mean/admissibility/mesh rules; initial-set gate and no duplicate/start validation; completed-only credit. | **P.** Scientific model/acceptance rules are present. Exact anchor identity and detailed callback gate are only implied/pending; bind them to frozen artifacts. |
| 8.10 / starts, S | Eligibility in both spaces and levels; preserve eligible pilot starts, deterministic replacements/reasons; never select on objectives/method outcomes. | **P.** Levels/spaces and preservation are stated. Explicit outcome-independent selection and every rejection record are missing. |
| 8.11 / cost | 120 shared cold-validation units and 60 checkpoint; actual cost/no L0 discount; raw seconds, solve counts, normalized cost and failures. | **P.** Budget/no-discount/failures are explicit. Solve-count and raw/normalized reporting need specification. |
| 8.12 / cost | No cross-method training cache/trajectory reuse; same within-run cache policy. | **M.** Add this fairness rule. |
| 8.13 / termination | Same CPU/thread limits and host policy; no selective restarts after ordinary termination; censored targets have no finite speedup. | **P.** Host policy and censoring are stated. Common thread limits and no selective extra restarts need explicit text. |
| 8.14 / fidelity | For D/E retain paired L0/common-validation F, all current stds and full-current mean for every validation, including rejected acceptance. | **M.** The draft describes frozen-point diagnostics only. This is a distinct all-validated-design fidelity dataset. |
| 8.15 / post-run | Best common-validated feasible point, earliest-validation tie; else last buildable training incumbent with no-feasibility label; else unavailable. | **C.** Comparison subsection states the full fallback chain. |
| 8.16 / post-run | At frozen point solve nonlinear L0 and common validator at .25/.75, g20; retain waveform/mean/std differences; no fit or trajectory modification; separate costs. | **P.** Solves, waveforms, means/stds and no feedback are explicit. Explicit paired differences and mesh/source provenance should be identified in the diagnostic schema. |
| 8.17 / refinement | Final candidates require L3/g40/interface checks; qualify claimed benefit if checks disagree. | **C.** Final-checks subsection states both. |
| 8.G1 / gate 1 | Every run records resolution/source/mesh/material/current/angles and requested/actual coordinates; assert identical common validator/anchor. | **P.** Generic pending source/data marker is insufficient. Identify exact manifest fields and coordinate-bit contract in supplement. |
| 8.G2 / gate 2 | Real L0 mesh, no L1 substitution; tests prove validation unchanged by training-resolution change. | **M.** Include reviewed wiring evidence among prerequisites; do not claim it from an all-L0 smoke. |
| 8.G3 / gate 3 | At admissible frozen points, scalar and grouped L0 adjoints checked against directional central-FD step sweeps; retain failures; setup evidence, not optimizer timing; no outcome tuning. | **M.** Add the derivative-verification method and the eventual frozen evidence reference. |
| 8.G4 / gate 4 | Reuse reviewed safeguards/initial-set gate; all five methods in scheduler/ledger/failure/checkpoint summaries; test inadmissible-credit rejection. | **P.** Intended rules appear, but independent implementation evidence is not identified. Supplement/runbook prerequisite. |
| 8.G5 / gate 5 | Starts/constants/options/order/commands committed before outcomes; bounded smoke labelled wiring-only; independent review and host choice before execution. | **M.** Header comments are instructions to authors, not a reader-facing reproducibility record. Add an explicit artifact/preflight reference. |

## Final-selection and start addenda

| ID / source | Clause | Coverage / action |
|---|---|---|
| F1 / representatives + correction | Ten possible representatives: minimum F among eligible uncontaminated runs, common mean floor AND h/discrete/mesh AND completed within budget. | **P.** Minimum and “eligible feasible” are present. Explicitly restate this conjunction and exclude contaminated attempts; do not rely on informal feasibility wording. |
| F2 / representatives | Tie by start, repeat, validation ordinal; earlier records from later failures remain eligible. | **C.** Both explicit. |
| F3 / representatives | Explicit missing representative; optional buildable-only diagnostic receives no credit. | **P.** Missing outcome is explicit. Optional diagnostic/no-credit distinction is absent; either omit that optional output entirely or label it exactly if used. |
| F4 / records | Archive every considered validation and reason selected/rejected; preserve provenance on dedup; never substitute after refinement failure. | **P.** Dedup roles and no substitution are explicit. Complete consideration/rejection records are missing. |
| F5 / crossings + correction | Up to 30 earliest repeat-1 crossings across starts, original target/margin rule and same tie-break; missing repeat1 never replaced by repeat2. | **P.** Repeat restriction/count/missing rule are explicit. “Earliest” lacks its measured cost field and the tie-break is not expressly applied to crossings. |
| F6 / matrix | Theta0, representatives and crossings at L3/h93/g20 and L2/h93/g40 for three training currents, plus .5 and .25/.75 diagnostics. | **C.** All named in final-checks subsection. Actual results remain pending. |
| F7 / matrix + I1 | Conforming outer-band Arkkio only at zero stator offsets, fresh meshes, supported rotations; other designs report model-dependent/no conforming check. | **C.** Restriction and limitation explicit. Nonzero rotor offsets are not excluded by this rule alone. |
| F8 / claims | Post-run costs separate; timing remains common L2; discrepancies qualify claims; selected points do not certify every crossing. | **C.** Explicit, including the useful extra typical-start limitation. |
| S1 / eligibility | Both spaces at L0/L1, actual strict/discrete guards, admissibility, no inversion and min angle ≥5 degrees on moved theta0 meshes without repair. | **P.** Model/space/moved-only rule is explicit; numeric angle and zero-inversion tests are hidden behind “policy.” State them or cite the frozen policy. |
| S2 / records | Archive every attempted index/rejection; geometry/mesh-only preflight, no FEM, cost outside run budgets. | **M.** Selected/replacement indices are mentioned, but full rejected-index inventory, no-FEM preflight and its cost boundary are not. |
| S3 / sampling + rationale correction | Moderate reference-mesh-reachable deformations, not uniform domain samples; later iterates may repair; do not claim broader eligibility inherently destroys pilot pairing. | **C.** Draft states the sampling limit and does not repeat the withdrawn rationale. |

## Inherited rules necessary to interpret this draft

These remain governing where A7/A8 did not supersede them. This table exposes
missing inherited context; it does not revive the pilot's 60-unit main budget,
three-method matrix, or obsolete target-selection rule.

| Source | Clause | Coverage / action |
|---|---|---|
| [I1](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6241473250), [I2](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6242081560) | Coupling torque primary; fixed LENGTH=.1 m, SD1=.135 m, ScaleR=1, OPERATING_ANGLE=0; eight named scalar variables and ten named groups; Opt120 outside this space. | **P.** Counts and qualitative fixed settings appear; exact constants/names/box and Opt120 distinction need a referenced table. Coupling torque omission is also 8.1. |
| I2 | Fixed-current primary versus labelled fixed-Jinit control; sizing/control studies are separate. | **P.** Fixed current is explicit; reference/control comparison scope is not. Clarify if the reference-design control appears elsewhere in the manuscript; do not silently add it to the 100-run matrix. |
| [I3](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6244183061), [I4 adoption §6](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6244746246), [I5 §4](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6245006308) | N simultaneous single-thread calibration processes, three cold validations each; unit is median of all 3N, not median of medians; retain single-process diagnostic. | **M.** “Measured cold-validation units” omits the defining estimator and concurrency. High-priority Methods addition. |
| I3 | Every candidate validation cold-starts and depends only on candidate; solve-count FE is secondary using measured training/adjoint weights, not the stopping rule. | **M.** Cold calibration does not state cold candidate validation. Add both distinctions. |
| [I4 §1/adoption](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6244506597) | Explicit unit-box and normalized objective/mean equations; training constants once/session and outside run budgets; training-infeasible iterates still validated. | **P.** Normalization is described but equations/constants boundary and training-versus-validation feasibility distinction need clarification. |
| I4 §2, [I5 §2](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6245006308) | C cubic RBF+linear tail of three stds and full-current mean; initial n+1 set; trust-region subproblem; feasible-first acceptance; rho/radius update; geometry points/rejection counting; near-duplicate handling; radius stop. | **P.** Model class/trust region/order appear. Algorithm and frozen constants are absent. Cite a complete algorithm/settings supplement, including A7's amendments; do not reconstruct it from the method label. |
| I4 §3 | SLSQP ftol=1e-15, gradient on each function evaluation and line-search accounting; COBYQA scipy1.18.1, 2n+1 initial set, radii .1/1e-6. | **M.** Required versioned optimizer-options table, shared by A/E and B/D. |
| I4 §5, I5 §3 | Kill in-flight request process group at budget and charge exact remainder; preserve aborted category/coordinates; normal convergence versus budget versus real error. | **P.** Completed-only credit and charged costs appear; termination semantics/taxonomy are not explicit. A7 mesh-infeasible restoration is a further distinct outcome. |
| I4 §6 | SplitMix64 ten-point, 18D LHS seeds1–4 under amended eligibility; bitwise frozen starts. | **P.** Ten-point/18D sequence appears; generator and exact vectors need a reference. |
| I5 §1 | B/C initial sets complete at 2n+1/n+1; then validate non-start incumbent once; changed incumbents and eligible final point; no duplicates. | **P.** Broad gate/start/duplicate exclusion appears, but initial-set sizes, completion boundary and final-point rule are not stated. |
| [I6](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6245012709) | C's .5-current checks test new designs/finer model, not unseen current; each stopped C run checks final incumbent at .25/.75 on training and validation models; compare predicted std and mean. | **P.** Empirical label and interpolation caveat appear. Distinguish C's final-incumbent selection and its two assessment datasets from D/E's different fallback selection. |
| [I6 addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6245148381) | Nearest-current linear interpolation of C std; extra post-run mean RBFs at 0/.5 fitted to same training data; never affect optimization; no intrinsic current-generalization claim. | **P.** Linear interpolation/no nonlinear solve stated. Neighbor intervals, post-run mean models and exact generalization limitation need a supplement reference. |

## Reviewed scope and operations details

| Source | Clause | Coverage / action |
|---|---|---|
| R18 | No CMA-ES/all-algorithm superiority, no intrinsic effective-dimension claim. | **C.** Final paragraph states these limits. |
| R18 | Full-current floor only; every final candidate's half-current mean compared against theta0; no partial-load guarantee. | **P.** Floor/diagnostic/limitation are explicit. State the theta0 comparison, not only that half-current is reported. |
| R18 | Numerical bounds: magnet area [20,300] mm², theta0 133; continuous gap >0.32876 mm, outer web ≥0.5 mm; no fixed volume/manufacturing/every-feature/mesh guarantee. | **P.** Limitations explicit; actual bounds and derivation/review reference absent. Cite the existing geometry-bounds appendix if these limits support the design claim. |
| R18 and #1202 | Measured user/system CPU for self and children with per-request/per-run boundaries; platform units/unavailable states; RSS lifetime high-water observations, no sum of maxima; wall time remains budget. | **P.** General process scope and RSS warning appear. Measurement boundaries, child-reaping/incompleteness, raw units/unavailable values and no nested CPU double counting need the resource-method supplement. |
| [Runbook](main-run-runbook.md), #1202 item8 | Clean calibration before dispatch; durable queue/attempt/load records; output overwrite refusal; preserve interrupted/completed attempts; explicit recovery disposition; immutable source/calibration provenance. | **M in fragment; present as operations requirements in runbook.** Link the runbook/recovery evidence in reproducibility materials. This is an implementation-readiness requirement, not a new optimization rule or a claim that resume already works. |

## Recommended incorporation order

1. Add concise main-text explanations of the unit under concurrency, equal
   optimizer settings across resolutions, non-adaptive coarse physics,
   all-validation fidelity records, L0 gradient verification, mesh-failure
   taxonomy, and exact representative/crossing eligibility.
2. Create or bind a frozen supplement for geometry inventory/scales,
   optimizer algorithm/options, initial-set/refusal/repair mechanisms,
   starts and host thresholds, CPU/RSS scope, and artifact/attempt schemas.
   It should carry the exact merged source and data hashes, not current
   provisional implementation heads.
3. Bind the existing runbook and resource methods to reviewed implementation
   evidence. Cite calibration, mixed-resolution smoke, derivative sweeps,
   refusal regressions and recovery checks before running the main study.
4. Fill actual host/environment/calibration/outcome/refinement records only
   after execution. Then revise the manuscript in a new immutable version,
   resolve the figure-5 mean wording, and obtain fresh reviews and audit.

The current draft has no main experimental outcomes, so none of these
coverage ratings establishes a speedup or completed publication readiness.
