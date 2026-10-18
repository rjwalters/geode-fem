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

Pending dependencies: #1201 safeguards and #1202 five-method integration. Raw
trace adaptation, the L3/g40/tight/shifted/conforming runner, resource accounting, result
summaries and a real wiring smoke remain to be implemented after those merge.
This module alone does not complete #1203 or validate any scientific claim.
