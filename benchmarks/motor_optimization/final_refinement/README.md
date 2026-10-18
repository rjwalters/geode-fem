# Final-refinement selection (preparation for #1203)

`selection.py` implements only the pure selection contract adopted in
[#1185's addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247383821)
and its [clarification](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247421487).
It is not yet connected to the experiment harness and cannot run FEM checks.
All tests use synthetic records; they provide no motor-performance evidence.

Run the stdlib checks from the repository root:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/test_selection.py
python3 -I benchmarks/motor_optimization/final_refinement/mutation_checks.py
python3 -I benchmarks/motor_optimization/final_refinement/test_check_plan.py
```

The suite has eighteen tests. `mutation-results.json` records the baseline and
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

Dependency #1201 has merged at `5c998bda`; #1202 five-method integration
remains under review in PR #1206. Raw
trace adaptation, the L3/g40/tight/shifted/conforming runner, resource accounting, result
summaries and a real wiring smoke remain to be implemented after those merge.
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

The raw-trace adapter, actual evaluator/refinement execution, conforming-interface
gate, actual cost collection and native wiring smoke remain pending dependency
integration. These synthetic checks supply no optimization or refinement result.

The read-only [adapter contract](adapter-contract.md) maps the current trace
and recovery fields to this API, bound to the source hashes in
`adapter-source-contract.json`. It identifies the credit-margin distinction
and the need to obtain fresh primary waveforms for refinement. It does not
implement the adapter or replace the required merged-source reconciliation.
