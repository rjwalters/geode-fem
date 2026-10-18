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
```

`mutation-results.json` records the baseline and sixteen semantic mutations,
including exact source/test hashes and unedited subprocess output. These checks
target omitted eligibility rules, timing/selection swaps, repeat substitution,
and coordinate rounding. They do not test the pending raw-trace adapter or FEM
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
method/space/start/repeat slot after contamination reruns.

Pending dependencies: #1201 safeguards and #1202 five-method integration. Raw
trace adaptation, the L3/g40/conforming runner, resource accounting, result
summaries and a real wiring smoke remain to be implemented after those merge.
This module alone does not complete #1203 or validate any scientific claim.
