# Final-refinement selection (preparation for #1203)

`selection.py` implements only the pure selection contract adopted in
[#1185's addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247383821)
and its [clarification](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247421487).
It is not yet connected to the experiment harness and cannot run FEM checks.
The Python selection tests use synthetic records. The native executor described
below can run FEM checks, but is not yet connected to a closed main-study cohort.

Run the stdlib checks from the repository root:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_selection.py
python3 -I benchmarks/motor_optimization/final_refinement/mutation_checks.py
python3 -I benchmarks/motor_optimization/final_refinement/test_check_plan.py
```

The original selection evidence below covered eighteen tests; later increments
add matched-reference tests. `mutation-results.json` records the baseline and
seventeen semantic mutations,
including exact source/test hashes and unedited subprocess output. These checks
target omitted eligibility rules, timing/selection swaps, repeat substitution,
coordinate rounding, and mixing reruns with disjoint validation ordinals.
They do not test the pending raw-trace adapter or FEM
runner.

The selector returns ten representative cells and thirty earliest-crossing
cells, including unavailable outcomes. Every considered validation retains its
source trace digest, metadata, eligibility reasons and selected roles. Earlier
validations of failed runs remain eligible. Only repeat 1 supplies crossings.
Deduplication preserves complete geometry/settings identities and every source
role; it uses float bits, not rounded printed coordinates. The returned cohort
never takes refinement results as input and never replaces a failed selection.

Before integration, the #1203 adapter must verify raw trace/anchor hashes,
common-validator settings, all geometry/discrete/mesh decisions, charged times,
run contamination/replacement eligibility and the complete physical geometry.
It must reject missing evidence rather than infer eligibility from optimizer
success or historical pilot flags. It must preserve all records, including
eligible records that were not selected. `Design.from_values` needs **all**
geometry and physical/numerical check settings; the selector cannot infer omitted
parameters. The adapter must also reconcile one eligible run per registered
method/space/start/repeat slot after contamination reruns. The selector rejects
otherwise eligible validations from two distinct run IDs in the same logical
slot, even when their validation ordinals differ. Multiple validations within
the chosen attempt remain eligible; representative and crossing selection can
choose different validations from that attempt.

`check_plan.py` prepares the all-mode schedule for the selected cohort under
the [part-2 addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6248492318).
`make_plan(selection_manifest)` binds the full selector manifest by SHA-256,
preserves exact identities and roles, and assigns each candidate a theta0
reference with identical requested check settings. Every design receives:

| Check | Spatial level / harmonics | Angles (degrees) | Newton relative tolerance | Current fractions |
| --- | --- | --- | --- | --- |
| Primary comparison | L2 / h93 | 0:1:19 | 1e-8 | 0, 0.5, 1 |
| Spatial refinement | L3 / h93 | 0:1:19 | 1e-8 | 0, 0.5, 1 |
| Angular refinement | L2 / h93 | 0:0.5:19.5 | 1e-8 | 0, 0.5, 1 |
| Tight nonlinear solve | L2 / h93 | 0:1:19 | 1e-12 | 0, 0.5, 1 |
| Independent shifted sweep | L2 / h93 | 0.5:1:19.5 | 1e-8 | 0, 0.5, 1 |
| Withheld-current diagnostic | L2 / h93 | 0:1:19 | 1e-8 | 0.25, 0.75 |

Each current sweep starts cold, then continues from the preceding angle.
All scheduled jobs require complete sweeps with retained raw waveforms; g40
subsamples cannot satisfy the independent shifted check. Primary records can
be reused only after the eventual runner verifies exact provenance/settings
equivalence. Half-current is already included in each training-current check.
The withheld check is the fine-model diagnostic, separate from #1202's paired
L0/common-validator diagnostics. No cross-current waveform subtraction or
withheld-current objective normalization is defined by this schedule.

Eight additional synthetic tests cover the matrix, references, source binding,
preserved cohort/credit, explicit non-execution and invalid identities. They
do not verify any actual FEM invocation. The plan intentionally leaves the
conforming-interface check pending: a runner must inspect actual stator
offsets, build fresh meshes and verify supported rotations before choosing
the estimator. Unknown or nonzero offsets cannot imply conforming eligibility.
Nonzero rotor offsets alone do not exclude an otherwise supported check.

Dependency #1201 merged at `5c998bda`; #1202 five-method integration merged
through PR #1206 at `9486a9da`. This branch incorporates that reviewed merge;
all 22 adapter source bindings matched the previously tested candidate byte for byte
at reconciliation. The native post-run executor subsequently adds observation-only
repair-history retention to its copy of the validator; it does not change the
source pin of the main-study producer.
The complete archive adapter and final-cohort execution remain pending. The
native executor below implements the refinement configurations and resource
accounting; its bounded wiring smoke is separate from full matrix validation.
This module alone does not complete #1203 or validate any scientific claim.


`comparison.py` prepares post-run reporting as a pure function:
`summarize(selection_manifest, declared_records)`. It reconstructs the frozen plan,
refuses unknown/duplicate jobs, and checks declared geometry identity, raw-data hash,
actual requested settings, complete current/angle sweeps, source/binary/common-physics
identity and separate cost scope. Each successful record supplies raw torque arrays
and reported means/stds/sum of variances. The reducer independently computes population
standard deviations and the registered three-current objective `sum(std**2)`.
Inconsistent aggregates or mismatched settings/provenance are errors, never silently
accepted comparisons. The future adapter still must establish these declarations
from actual files and execution evidence; this API does not verify a hash against
an external raw file or prove that FEM ran.

Every missing planned job remains `not_run`. Failed, mesh-infeasible and unavailable
observations keep their source records/costs, and dependent comparisons remain
unavailable. No record can replace a selected design or overwrite an earlier check.
The complete original cohort and roles remain in the report; input records and
selection are not mutated. Every output binds the selection, generated plan and
observation dictionary by SHA-256.

Comparisons report signed candidate-minus-reference changes in means, standard
deviations, objective and torque samples. Waveforms are subtracted only at identical
physical angles: g40 has the twenty primary angles in common, while shifted g20 has
none and therefore no primary-grid waveform difference. Its aggregate changes and
raw shifted waveforms remain available, as do full waveform differences from theta0
on the same shifted grid. No interpolation or index-based subtraction is implied.
The module never substitutes g40 samples for an independently executed shifted sweep.

Target-survival diagnostics use theta0 at identical requested refinement settings,
the registered `F + max(0.1 F, 2.5e-7)` margin, full-current mean floor and explicit
geometry/discrete/mesh checks. Their scope is post-run; original common-validator
credit and attainment times stay unchanged. Per-mode feasible candidate orderings
expose rank changes without reselection. Withheld-current waveforms receive mean/std
comparisons, but their sum of variances is not labelled as the registered objective
and earns no target credit. CPU/RSS and elapsed costs are retained per observation;
the reducer does not add overlapping measurements.

Ten synthetic comparison tests and five mutation checks pass, alongside eighteen
selector and eight plan checks. Tests cover changed orderings, matching-setting
references, shifted-angle alignment, withheld currents, failures/missing jobs,
source/setting/aggregate mismatches and preserved cohort/credit. Run with:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_comparison.py
python3 -I benchmarks/motor_optimization/final_refinement/comparison_mutations.py
```

The complete raw-trace adapter and full-cohort execution remain pending.
The native executor below supplies the execution and conforming gate; these
synthetic comparison checks supply no optimization or refinement result.

The read-only [adapter contract](adapter-contract.md) maps the current trace
and recovery fields to this API, bound to the source hashes in
`adapter-source-contract.json`. It identifies the credit-margin distinction
and the need to obtain fresh primary waveforms for refinement. The generation reader below implements its file-binding layer; the complete
adapter remains pending; source hashes have been reconciled against the reviewed merge.

`geometry_evidence.py` is a pure preparation helper for the future adapter. It
checks complete declared guard sets, finite normalized margins, construction
and discrete verdicts, and agreement between exact and tolerance-aware flags.
It uses credit `h >= -1e-12`, so a validation with `h = -5e-16` remains eligible
even when `h_nonnegative` is false. Six synthetic checks exercise that mapping
through representative/crossing selection, the inclusive boundary and next
float below it, construction failures, absent/duplicate guards, nonfinite
margins and inconsistent flags. Run:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_geometry_evidence.py
```

This helper trusts the supplied guard definitions and declared construction
verdicts. It does not bind files to hashes, reconstruct geometry, verify margin
normalization/scales, validate a mesh, reconcile attempts or grant whole-run
eligibility. Those remain full-adapter responsibilities. No FEM ran in these
tests and no main-study evidence is implied.


`trace_archive.load_generation(path)` reads an actual generation directory
without opening the recovery registry or writing files. It checks the 100-job
rotation/reversal schedule, readiness/frozen-input/terminal-trace hashes,
common unit, accepted calibrations, job/session identities, observed training
and validation meshes, and validation ordinals. Every attempt and its raw trace
remain available, including contaminated runs, scientific failures, unfinished
workers and explicit missing traces. Copied archives retain the original source
directory identity alongside their local location. Duplicate JSON keys, nonfinite
numbers, torn journals, active local locks and redirected files are refused.
All bytes consumed are hashed again before returning.

This is the input layer, not final cohort eligibility: it explicitly returns
`can_select=false` and `can_launch=false`. It does not prove remote process
absence from a copied lock, resolve recovery generations or external attestations,
or check canonical anchor/source/runtime provenance, geometry, mesh quality,
mean floor, target flags or completed validation times. These checks remain
necessary before feeding records to `selection.py`. An absent final session
manifest or unfinalized attempt stays visible, without granting completion.

Nineteen on-disk synthetic tests and twelve assertion-detected mutations cover
these boundaries. A separate schema probe uses the actual #1206 queue builder
and `run.py` writer with a synthetic evaluator and bounded mock optimizer
prefix: all five emitted traces load, all 100 queue entries agree, and source
files remain byte-identical. It is neither a FEM smoke nor main-study evidence.
`trace-archive-checks.json` binds the scripts and compressed raw evidence.

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_trace_archive.py
python3 -I benchmarks/motor_optimization/final_refinement/archive_mutations.py
```

`validation_evidence.verify_validation` checks the next layer: recorded geometry
margin normalization and strictness against frozen guard definitions, the common
current set, objective/mean aggregates, accepted mesh quality and repair history,
charged wall-FE/seconds, the mean floor, and all three target flags. The inclusive
5-degree mesh threshold and `h >= -1e-12` credit boundary are preserved. A fresh
primary mesh must follow a failed moved-mesh policy and end in a passing attempt
with the same recorded digest. A mesh-infeasible observation retains all four
failed attempts and earns no target credit. Unknown or contradictory evidence
raises an error rather than silently becoming an eligible record.

`adapt_validation` feeds those checked values into `selection.Validation`, using
an independently reconciled run context and a complete, bound `Design` supplied
by the caller. It retains earlier valid observations of a later-failed run,
post-budget observations, and contaminated/unknown-host observations with their
selector rejection reasons. Numerical attainment and within-budget target credit
are separate: the original numerical flag can remain true after the budget.
Neither this adapter nor its tests resolve whole-study eligibility.

Arithmetic consistency permits eight binary64 ulps for JSON round trips and
recomputed aggregates; guard subtraction propagates the rounding of its inputs.
The actual budget, guard, mean and target inequalities are exact, without those
tolerances. A disagreement between the recorded FE-budget inequality and its
seconds equivalent is refused for explicit reconciliation. The caller supplies
the cohort's verified effective anchor, after the registered canonical-file
parser check, rather than silently substituting a new platform's recomputation.

Twenty tests and twelve assertion-detected mutations exercise these rules and
their path into selection. A separate probe replays retained native L2 theta0
validation values with eleven guard definitions from a fresh geometry-only
query; no new FEM solve runs. It correctly excludes theta0 for its mean floor.
Its charged stamp and reconciled context/design are synthetic, so this is a
schema/arithmetic check, not an optimization or refinement result. The raw
records and source hashes are retained with `validation-evidence-checks.json`.

Remaining caller obligations include complete recovery-chain reconciliation,
raw file/source/runtime/anchor bindings, actual geometry reconstruction from
coordinates, full design/settings identity, and verification of referenced mesh
bytes. Recorded quality and matching mesh digests alone do not discharge those
obligations. The native runner below implements execution; the complete driver
and independent review remain pending.

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_validation_evidence.py
python3 -I benchmarks/motor_optimization/final_refinement/validation_mutations.py
```


`recovery_evidence.reconcile` loads the original archive and supplied recovery
registry without constructing a registry or dispatching work. It verifies origin,
reserved plans, generation links, seals and original frozen inputs. Every reserved
optimizer/diagnostic action is checked against the prior history using the actual
hash-pinned `recovery_policy.plan`, executed in an isolated subprocess from verified
source bytes. A stale bytecode cache cannot replace that source. All three policy
source dependencies must be pinned by the caller.

The first clean completed optimizer remains selected, including scientific failure;
later outcomes do not replace it. Replacement allowances carry across generations.
Unfinalized traces, unresolved interruptions, exhausted replacements, incomplete
registry seals and pending diagnostics remain explicit. Diagnostic-only repairs
bind the complete original optimizer/snapshot identity and cannot add an optimizer.
Copied external-attestation evidence is hash-checked and remapped only in memory;
its original absolute paths remain in the returned record.

`all_registered_jobs_resolved` describes the supplied chain only. The reader still
returns `can_select=false` and `can_launch=false`. A copied archive cannot prove
remote process absence or that no later generation exists. The caller must establish
capture completeness, externally reviewed interruption causes, complete source/runtime
and canonical-anchor provenance, and full geometry/settings/mesh identities before
scientific selection. This layer supplies attempt dispositions to that later adapter.

Seventeen tests exercise these rules. A production schema probe creates a private
synthetic study using the actual Registry writer, inventory and recovery policy.
Its 100 completed jobs retain 101 attempts, including one contamination replacement;
the entire resulting policy matches the reader, and archive bytes/modification times
remain unchanged. No optimizer or FEM solve runs. Nine semantic mutation checks and
the raw production-probe archive are recorded in `recovery-evidence-checks.json`.
Recovery tests now use the merged harness in this checkout by default:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_recovery_evidence.py
python3 -I benchmarks/motor_optimization/final_refinement/recovery_mutations.py --harness benchmarks/motor_optimization/harness
```


Matched references are now explicit. `select(..., theta0=..., theta0_references=...)`
accepts additional theta0 entries with the same exact physical parameters and
separate complete settings. For the real adapter these settings must distinguish
space A's scalar motion map from space B's control-point map, even with zero
offsets: knot-parameter attachment and harmonic-extension weights differ from
the scalar map. Matching scalar values therefore do not justify cross-space
deduplication. Supply both per-space references; both remain in the check cohort
even if one space has no selected candidate.

Each selected design records `theta0_design_key`. A missing matching-settings
reference is an error. `make_plan` independently checks reference geometry and
settings, emits the per-job `base_settings_sha256` alongside each refinement
specification, and schedules all references before candidates. Each candidate's
six checks point to its matching reference at the same resolution, angle grid,
current set and tolerance. `comparison` refuses cross-settings waveform subtraction,
uses the declared matching reference for target-survival diagnostics, and keeps
all reference entries out of candidate rankings. A missing reference remains
unavailable; the other space's result cannot substitute for it.

The original common-validation F0/mean0 and all selection credit remain unchanged.
These extra theta0 checks are post-run evidence, not new optimization trajectories
or a change to target thresholds. Seventy relevant tests pass, including eight new
selection/planning/comparison checks. Seventeen selector, five comparison and six
matching-reference mutations are detected. `matched-reference-checks.json` binds
this evidence and a synthetic four-design, 24-check example. The motion distinction
is established by the source implementation. The coordinate adapter and native
executor below now implement those parts; complete main-capture integration is pending.


`design_evidence.RegisteredDesigns.load` reads the reference and frozen-start
files against supplied SHA-256 values, checks all twelve scalar registrations,
the four frozen values, coordinate order, boxes and both theta0 vectors.
`adapt(trace, validation, provenance=...)` checks the native ready protocol,
full parameter-name order, space/map and common L2/h93/g20 validator settings.
It expands the free coordinates to twelve scalars plus ten explicit offsets,
returns their exact bits in native order, and builds both the candidate `Design`
and its matching-space theta0 reference. Coordinates are never projected,
rounded or inferred from an objective. Endpoints are accepted; missing,
nonfinite, out-of-box or contradictory-bit records are refused.

Final-check identity includes the originating space/motion map, complete
parameters, observed common-validator settings/reference mesh, reference-data
hash and declared source/binary provenance. Optimizer identity and training
resolution do not change that final-check identity. The two spaces retain
identical physical theta0 parameters and distinct numerical identities. The
reported zero-stator-offset predicate is only one prerequisite for a conforming
check: it grants no permission without the native fresh-mesh/rotation gate.

Eleven tests and ten assertion-detected mutations pass. A real evaluator probe
checks readiness and geometry at all five frozen starts in both spaces, with
zero FEM/optimizer solves. A separate composition replay maps retained native
theta0 coordinates/values through the design and validation adapters, selector
and matched-reference plan; it preserves mean-floor exclusion and both reference
sets. Its run/host/timing context is synthetic. `design-evidence-checks.json` binds
both probes, scripts and raw data.

This maps the complete registered parameter vector; it does not reconstruct
curves or mesh nodes or authenticate the caller's source/build/runtime claims.
The caller must bind these declarations to the verified main capture, canonical
anchor, actual mesh bytes and independently reviewed execution. Native refinement
and conforming checks are implemented below; complete end-to-end main-data
adaptation remains pending. Run the local checks with:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_design_evidence.py
python3 -I benchmarks/motor_optimization/final_refinement/design_mutations.py
```

### Native post-run executor

`icem_final_refinement` executes one check of one exact design per process:

```sh
cargo build -p geode-core --example icem_final_refinement --release
./target/release/examples/icem_final_refinement --request request.json --mesh theta0_L2.msh --out NEW_DIRECTORY
```

Use the worktree's private Cargo target directory (or its Loom-owned target).
The output directory must not already exist. Each attempt retains the request,
`result.json`, and any fresh meshes and mesher output. Scientific failures return
exit status 2 with their result; they never select a replacement candidate.

The request has exactly these fields:

- `schema: "icem-final-refinement/1"`, `cost_scope: "post_run"`, `space: "a" | "b"`.
- `check`: `primary`, `L3`, `g40`, `tight`, `shifted`, `withheld`, or `conforming`.
- `design_key` and `base_settings_sha256`: caller-bound cohort/settings identifiers.
- `reference_data_sha256` and `reference_mesh_sha256`: hashes of the actual input bytes.
- `x_bits`: unsigned binary64 bit patterns of the eight or eighteen free coordinates.
- `point_names` and `point_bits`: complete native point order and exact expanded values,
  as emitted by `RegisteredDesigns.adapt` (`native_point_names`, `native_point_bits`).

The executable independently reconstructs the full point, rejects disagreement,
nonfinite values, out-of-box coordinates and unknown overrides, and checks the
geometry's construction/discrete constraints and credit tolerance. It selects
fixed registered settings, including repair and the 5-degree mesh policy.
`shifted` starts its own 0.5, 1.5, ..., 19.5-degree solve; it does not slice g40.
Each current starts cold and subsequent angles continue the previous solution.
Raw waveforms, current means/variances, Newton iterations, mesh evidence and
post-run wall/CPU/RSS measurements are retained. The main-run credit is unchanged.

Harmonic checks use the supplied theta0 mesh of their level and existing
moved/fresh policy. Conforming checks require exactly zero stator offsets and
supported rotations, then **always** generate a new candidate mesh, even for
theta0. Their separate fresh-mesh attempt records establish that origin; the
validation's `mesh.kind=own` means own geometry on that newly generated mesh.
Nonzero rotor offsets alone do not disqualify this check. An undefined conforming
check is recorded as unavailable/model-dependent. A fresh mesh must pass policy
before any solve.

This executable does not authenticate the cohort, source revision, binary or
runtime from caller-supplied identifiers. It also cannot infer from a mesh hash
that a supplied harmonic base mesh was built at theta0. Those bindings and the
closed-study capture must be verified by the complete driver. A solve error can
lack partial waveforms; no missing current or angle is counted as completed.
The validator also retains the completed repair history before a failed solve,
so the preceding mesh costs and quality checks remain inspectable. The bounded
coverage below exercises the matrix and both maps. Independent review and the
complete frozen-cohort driver are required before interpreting main-study outcomes.

`native-execution-checks.json` records the initial native wiring evidence at
`63b3e3d8`: six Rust tests, private recorded build, and real theta0 primary checks
in both spaces (all torques bit-identical to the committed reference). A separate
shifted-angle solve and a successful newly generated conforming mesh add 120
samples, for 240 raw torque samples in total. Wrong point bits, wrong mesh hash
and undefined nonzero-stator conforming inputs retain refusal/unavailable records.
The hashed archive includes the fresh mesh and build receipt. These are bounded
wiring checks; the remaining configuration coverage follows below. The complete
closed-study driver and independent review are still required.


`native-matrix-checks.json` completes the bounded configuration coverage on the
same recorded binary, without changing runner code. It retains 400 additional
samples from g40, tight, withheld, L3 and lhs1 in both spaces (640 with the first
archive). The scalar lhs1 uses mesh motion; CP lhs1 triggers and passes fresh
remeshing. Both have mean torque below the reference floor, so their small ripple
alone is not success. The regenerated L3 mesh matches its committed hash, and
the g40/L3 waveforms match the existing reference records bit for bit.

At theta0, relative changes in the registered sum of variances versus primary
are `+2.46764e-6` (g40), `+8.36056e-8` (tight), `+5.25812e-6` (independent shifted)
and `-0.0127382` (L3). These are numerical sensitivity checks at the reference
design, not optimizer gains. Withheld-current sums remain separate from that
objective. The archive includes raw waveforms, fresh meshes, generation logs,
source bindings and exact reference comparisons. Main-capture authentication,
frozen-cohort execution/reporting and independent review remain pending.

`native_results.py` now joins exact request preparation, retained native files and
comparison reporting for the six harmonic checks. `requests(...)` verifies the
cohort's complete designs/settings and the supplied L2/L3 mesh bytes, then emits
requests identical to the native format. `report(...)` binds each result and its
retained request, checks actual coordinates/configuration, independently checks
geometry margins, mesh quality/history and retained mesh bytes, and recomputes
waveform statistics. CPU counters and post-run cost scope remain per observation.
Failed/unavailable results and missing checks are retained without replacement.
The conforming integration described next extends this same API.

The nine tests in `test_native_results.py` replay nine unchanged native harmonic
outputs (580 torque samples) from the two existing smoke archives. The additional
compressed L2 fixture supplies the exact original input mesh, making the replay
independent of external worktrees. **Selection context is synthetic:** both lhs1
points are deliberately included with invented prior feasible selection values;
the report correctly rejects their actual final means and retains 15 unperformed
checks. This exercises reporting, not optimizer performance or a main cohort.
Mutation cases cover identities/settings, files, geometry, complete waveforms,
aggregates, mesh history, resource totals, failure retention and out-of-plan data.
All 126 final-refinement Python tests pass.

The caller must still authenticate complete closed-study capture, source/build/
runtime receipts, expected geometry definitions and base-mesh construction. File
hashes and declared provenance alone do not establish those facts. This API does
not select main candidates or launch processes; both capability flags are false.

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_native_results.py
```


The frozen plan, comparison reducer and native reader accept
`include_conforming=True` to add one conforming check for every candidate and
matching-space theta0 reference. The six-check default remains available for
harmonic-only archives; it does not imply that conforming checks are complete.
The added specification declares L2, the outer-band Arkkio estimator, a fresh
candidate mesh, zero stator offsets and supported rotations. Every requested
check remains in the report even when its estimator is unavailable.

A successful conforming output must retain its ordered fresh-mesh attempts,
actual generated files, final passing quality and the same mesh in validation.
The reader refuses moved-mesh substitution, mismatched quality/digests, skipped
repair history, nonzero stator offsets and unsupported rotations. Rotor offsets
alone do not cause refusal. Conforming versus primary comparisons retain the
matched physical angles and disclose their different estimators; target checks
use the matching conforming theta0 and cannot alter original credit.

Six additional tests replay the original successful fresh conforming solve and
the native nonzero-stator unavailable result. The combined report has ten
completed checks, 640 torque samples and eighteen missing checks in its synthetic
four-design cohort. All 132 Python checks pass. No new FEM or main-study candidate
selection is performed. Full closed-capture authentication and cohort execution
remain pending; native build/source/runtime declarations still require external
verification.

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_conforming_results.py
```

The new `closed_capture.py` prepares a terminal study for offline verification.
It runs on the original host and paths, after all registered jobs and required
recoveries/diagnostics are resolved. It uses the producer's existing isolated
Python environment and original working directory; it launches no optimization,
FEM, build or recovery and does not rewrite the study or producer checkout.

`producer_source.py` verifies the complete declared source census against an
independently selected reviewed Git commit and the actual clean checkout. Producer
Python modules are compiled from those verified bytes, so stale timestamp-valid
bytecode cannot replace them. The exporter calls the production runtime/input
binding verifier, witnessed-build verifier, process verifier and pinned recovery
policy. It requires all 100 registered jobs resolved, a complete final manifest,
no active tracked owner/group, and unchanged source/runtime/input/host bindings.

The exporter copies the entire original/recovery trees, build records and binary,
frozen external inputs and its own tool sources into a new directory. Before/after
checks retain the actual process tables and reports, source/runtime observations,
complete file/directory censuses and reconciled policy. Every original path remains
in the evidence; a separate mapping locates its portable copy. New/changed files or
registry state invalidate capture. A failed copy retains `capture-failed.json` and
never publishes `capture.json`; an existing destination is refused.

The exporter and its three dependencies must be copied **outside** the frozen
producer checkout. After the study closes, the owner can run the following from
`/home/ubuntu/geode-fem`, with `CAPTURE_PYTHON` set to the same existing virtualenv
interpreter and the same launch environment used by the study:

```sh
"$CAPTURE_PYTHON" -I -B /home/ubuntu/exp2-capture-tool/closed_capture.py \
  --source /home/ubuntu/exp2-main/main \
  --repo /home/ubuntu/geode-fem \
  --reviewed-commit 9486a9da58beeca91968d4d3608f06b998abff45 \
  --out /home/ubuntu/exp2-main/closed-capture
```

The tool bundle is `closed_capture.py`, `producer_source.py`, `trace_archive.py`
and `recovery_evidence.py`. Use the existing environment directly; capture should
not install packages or rebuild a virtualenv to make an equality check pass.
The returned manifest SHA-256 must be retained with the observed execution receipt
before copying the export elsewhere. The capture is a witnessed local observation,
not a cryptographic signature or independent scientific review, and is time-bound:
it does not authorize later modifications or prove that no future run can start.

Sixteen new tests cover trusted Git/source binding, stale bytecode, source/index
changes, complete copying without source mutation, file/registry/host changes,
actual production process observations and the 99-versus-100-job closure boundary.
All 148 final-refinement tests pass. Runtime/build orchestration uses explicit
fixtures in these tests; no completed main capture has been made. A read-only
probe of the running study's frozen inputs separately matched all 1,419 declared
source hashes to reviewed commit `9486a9da`; this does not assert current remote
source/runtime equality, process absence or study closure.

The offline capture verifier and archive-to-selection driver remain to be joined
to this exporter and the existing scientific/design checks. Both selection and
launch capability flags stay false until the full workflow is verified.

`capture_archive.load` now verifies the copied export before scientific adaptation.
Its required inputs are the manifest SHA-256 retained from the capture execution,
the independently reviewed producer commit and the reviewed capture-tool commit.
The archive cannot choose those identities itself. The reader checks its complete
file/directory census, portable path mapping, source map against Git, captured
input bytes, build inputs/log/artifact/receipt/journal and before/after producer
observations. It executes no code copied from the archive.

Recorded process tables are replayed through the process verifier loaded from the
reviewed producer Git revision. Every generation and the build must be covered;
recorded journal hashes, host/boot, capture times and absence decisions must agree.
The recovery policy is also recomputed from the copied journals using its reviewed
source dependencies. All 100 jobs must still resolve, and the canonicalized policy
must equal the captured policy. The reader checks the complete census again before
returning. These are time-bound recorded observations, not a claim about the host's
current process state or a cryptographic execution attestation.

Fourteen new tests round-trip a synthetic 100-job study through export and offline
verification, using the real reviewed Git map, recovery policy and process verifier.
Build/runtime and optimizer evidence in this fixture are explicitly synthetic.
Mutations reject changed manifest identities, files, code/source declarations,
build bytes, producer observations, process tables/reports/times, policy and path
mappings, including removal of the hundredth terminal completion. All 162 Python
tests pass, and the successful reader leaves every archive byte and mtime unchanged.

The result has `capture_verified=true` within the separately witnessed-capture
trust boundary, but `can_select=false` and `can_launch=false`. Original setup and
calibration consistency, canonical anchor parsing, per-validation geometry/mesh/
credit/timing checks, full design adaptation and scientific selection remain the
next integration layer. A trusted digest must not be manufactured by hashing an
untrusted archive and then treating that hash as an independent capture receipt.

The synthetic archive preserves permission bits as well as bytes. Its extracted
replay passes when those modes are retained. Python's `data` tar filter changes
the fixture evaluator from mode `0555` to `0755`, which the complete census
correctly rejects. For this hash-pinned archive, validate that all members are
regular relative paths with ordinary permission bits, then use the `tar` filter
to preserve modes; `capture-archive-checks.json` records that packaged replay.


`setup_evidence.verify` checks the original frozen setup after capture verification.
It replays the reviewed producer's pure setup, anchor-comparison and calibration
functions from Git without importing its launcher or executing a solve. Both
motion-map anchors must agree, their waveforms must reproduce the aggregates,
and the canonical anchor remains the source of targets. The ready-record parse
uses the original driver's four-epsilon bound; the host recomputation does not
replace it. The session's anchor receipt and training-mesh paths remain bound.

All twenty frozen starting-design checks must retain their registered coordinates,
passing geometry and moved-mesh quality, and zero FEM charges. The two calibration
records must pass their host gates, contain three timings per worker, reproduce
all medians, weights and training values, and reuse the primary common unit.
Verified starting checks supply the geometry guard definitions for subsequent
validation-record checks. The result retains both matched theta0 designs.

The hash-pinned `smoke/live_frozen_setup_9486a9da.tar.gz` contains the real frozen
setup records copied read-only from the running main study. Twelve tests replay
these records and reject changed targets, waveforms, sources, start coordinates,
geometry, costs, calibration samples, units, training values and host gates.
This is a live frozen-setup schema check, not a closed main capture or optimization
result. The capture-entry wrapper has not yet been exercised on a complete real
scientific capture. Per-validation adaptation, generated-mesh byte checks and
archive-to-selection integration remain required; selection and launch flags
remain false.


`study_selection.load` now connects capture verification, frozen setup replay,
scientific record checks and cohort selection. It uses the reviewed recovery
policy's selected completion for each of the 100 logical jobs. It retains all
attempts and rejected validation records, including contaminated attempts and
earlier valid observations from runs that eventually stop with an error.

The adapter binds each trace to its original start, effective canonical anchor,
training configuration and full design. It replays the reviewed producer ledger,
requires validation stamps to match ordered ledger prefixes, and recomputes
geometry, mesh policy, feasibility, targets and budget credit. Every generated
validation mesh digest must match captured mesh bytes inside its own attempt's
validation directory. Reference mesh bytes were already checked during capture
verification. Missing generated evidence refuses selection; it is not silently
excluded in favor of a different candidate.

Selection emits all 40 cells, the full validation audit, complete selected-design
identities, both matched theta0 references, and the seven-mode final-check plan.
No solver runs and `can_launch` remains false. This freezes a cohort; it does not
establish refinement success or comparative optimization performance.

Run only after complete capture, with the separately retained capture digest and
independently reviewed producer/tool revisions:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/study_selection.py \
  --capture /path/to/closed-capture --manifest-sha256 "$CAPTURE_SHA256" \
  --git-repo /path/to/geode-fem --reviewed-commit "$PRODUCER_COMMIT" \
  --capture-tool-commit "$CAPTURE_TOOL_COMMIT" --out /path/to/frozen-selection.json
```

The output must be new and outside the archive. The command prints its SHA-256
and cohort/check counts. Thirteen integration tests include a complete synthetic
100-job capture through the public API and CLI. Real frozen physics definitions
and reviewed source computations are used, but build/runtime observations,
training-mesh bytes and optimizer outcomes are explicitly synthetic. All 188
Python tests pass. This test is not a completed main study; independent review,
real closed capture, selected-cohort execution and paper results remain pending.

The portable fixture `smoke/synthetic_scientific_selection.tar.gz` replays the
public API after extraction and compares its frozen selection, plan and complete
design mappings. Its pinned archive/capture digests and permission-preserving
extraction checks are recorded in `study-selection-checks.json`. Run its
`replay.py /path/to/geode-fem` with isolated Python. The fixture's independent
capture identity is test data, not a witness for real scientific execution.


The first parent-finalized main trace is retained separately in
`smoke/main_first_terminal_schema.tar.gz`. Its six validations and 1,377 ledger
entries replay against the original setup, source, anchor, starts and mesh bytes.
It exposed and corrected a reader assumption: native `SessionConfig::is_registered`
means equality with the original L1/L2 configuration, so the approved L0/L2
baselines correctly report `false`. The reader now checks that level-dependent
flag alongside the complete training and common-validation configuration.
Both D/E acceptance and wrong-flag rejection are regression tested; the real
trace replay is an additional test. This is partial live-study evidence only:
no closed capture, final cohort selection, target or speedup claim follows.
