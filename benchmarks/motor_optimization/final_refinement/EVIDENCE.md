# Issue #1203 — consolidated review evidence

This branch implements the frozen post-main selection, execution and reporting
pipeline. It does **not** contain a completed main-study capture, a selected main
cohort, or evidence of optimization speedup. Main execution and manuscript review
remain separate. Review the whole PR against main `9486a9da`, not the incremental
Squad requests that preceded it.

## Reproduce the current checks

From the repository root:

```sh
python3 -I -m unittest discover \
  -s benchmarks/motor_optimization/final_refinement -p 'test_*.py' -v
```

The test runner is stdlib-only after the four required meshes are regenerated
with pinned Gmsh/NumPy (see README and `smoke/REGENERATE.md`). It reads committed
historical Git objects, so use a full-history checkout. It replays hash-pinned data and starts controlled Python
children; it does not compile or execute archived native binaries. The dedicated
`motor-final-refinement` CI job uses Python 3.12 and full Git history.

The Rust target `motor_final_refinement` is included in both the build and test
steps of the core integration job. Its six tests were run at native commit `63b3e3d8`; the actual paired
release builds at `1c6784e7` contain the same native source. The current branch's
subsequent changes are Python orchestration, retained evidence, CI/docs and
removal of unused Rust imports; no native numerical logic changed. CI provides the PR-head Rust verdict.

## Acceptance and implementation map

All paths below are relative to this directory unless otherwise stated. The
`*-checks.json` records identify exact source hashes, historical commands, archive
hashes, outcomes and limitations. Historical test counts describe their own
increments; the full-suite command above is the current check.

| Requirement / boundary | Implementation and tests | Retained evidence and targeted mutations |
| --- | --- | --- |
| Completed within budget; mean, geometry, discrete, mesh and host eligibility; earlier valid records of later-failed runs | `selection.py`, `validation_evidence.py`; `test_selection.py`, `test_validation_evidence.py` | `mutation-results.json`, `validation-evidence-checks.json`; omit mean/geometry/discrete/mesh/contamination/completion gates, credit after budget, exclude exact boundary, discard earlier failed-run records, ignore normalization/repair hashes/current set/objective/margin |
| All 10 representative and 30 repeat-1 crossing cells; exact ties and missing cells | `selection.py`; `test_selection.py` | `mutation_checks.py`; repeat-2 substitution, crossing by objective, representative by time, repeat-before-start, missing relative/absolute target margin |
| Exact geometry/settings identities, all source roles, matching-setting theta0 | `selection.py`, `design_evidence.py`, `check_plan.py`; corresponding tests | `design-evidence-checks.json`, `matched-reference-checks.json`; rounded coordinates, changed frozen length, conflated motion maps, wrong coordinate bits/order, wrong/missing theta0 settings |
| Every completed/failed/contaminated attempt retained; exact reviewed replacement policy | `trace_archive.py`, `recovery_evidence.py`; corresponding tests | `trace-archive-checks.json`, `recovery-evidence-checks.json`; hash/queue/unit/training mismatch, hidden failures/contamination, last-clean-wins, unauthorized retry, diagnostic/snapshot binding, unsealed closure |
| Authenticated terminal capture and producer Git bytes | `closed_capture.py`, `producer_source.py`, `capture_archive.py`; corresponding tests | `closed-capture-checks.json`, `capture-archive-checks.json`; source/index/file census, live/torn process evidence, changed binary/input/runtime/journal, incomplete queue, copied-path and independently retained digest checks. Synthetic capture round-trip is explicitly synthetic. |
| Frozen setup, anchor/calibration, complete scientific selection | `setup_evidence.py`, `study_selection.py`; corresponding tests | `setup-evidence-checks.json`, `study-selection-checks.json`; changed anchor/target floor, calibration/unit/thread/load, coordinates, ledger charge/prefix, training/current models, selected completion and generated mesh bytes. `main-trace-schema-checks.json` replays one actual L0 terminal trace for schema only, not cohort selection. |
| Actual native grid/tolerance/current/initialization and conforming restrictions | `../../../crates/geode-core/examples/icem_final_refinement.rs` and `../../../crates/geode-core/tests/common/icem_final_refinement.rs` (repository-root paths are `crates/...`); Rust `motor_final_refinement`; `test_check_plan.py`, `test_native_results.py`, `test_conforming_results.py` | `native-execution-checks.json`, `native-matrix-checks.json`, `native-binding-checks.json`, `conforming-binding-checks.json`; real primary/L3/g40/tight/independently shifted/withheld/conforming outputs, nonzero stator refusal, supported-rotation/fresh-mesh gates, wrong request/config/point/current/mesh/torque/aggregate mutations |
| Complete frozen request bundle; no reselection or replacement | `refinement_bundle.py`; `test_refinement_bundle.py` | `refinement-bundle-checks.json`; wrong source/L3 construction/reference, changed plan/request bits/geometry query type, missing check, mesh/census mutation, existing output refusal |
| Source-bound geometry and final-check executables | `refinement_build.py`; `test_refinement_build.py`, `test_paired_build_archive.py` | `refinement-build-checks.json`; actual release compiler logs, binary digest/size records and source census, process absence, copied evidence replay; failed second compiler, wrong example, source drift, changed binary/name/census |
| Current runtime/input/source paths and build-process prerequisites | `refinement_environment.py`; `test_refinement_environment.py` | `refinement-environment-checks.json`; actual offline NumPy/Gmsh package/library observations and source/build/input verification; missing cache refusal, wrong package version, source/input/environment drift, live process, changed boot, output overwrite |
| Journaled full schedule, raw output and scoped costs, failures without retries | `refinement_run.py`, `refinement_exec.py`; `test_refinement_run.py`, `test_runner_native_archive.py` | `refinement-run-checks.json`; controlled complete run/finalization, scientific refusal, missing result, SIGTERM/reaping, wrong executable/query, missing schedule/journal, changed receipt/census; real bounded geometry + primary execution, 60 torque samples, historical process replay |
| Finalized execution to scientific summary; missing comparisons, unchanged credit and budgets | `refinement_report.py`, `native_results.py`, `comparison.py`; `test_refinement_report.py`, `test_comparison.py` | `refinement-report-checks.json`; real retained waveforms in synthetic execution/selection context, PID/exit/resource consistency, changed request/statistics, geometry failure, abnormal exit, missing outcome, finalization/digest/runtime/build/process mismatch. `comparison_mutations.py` covers wrong matched resolution, withheld sums mislabelled as objective, mean/mesh/settings gates. |

## What the native evidence establishes

`native_refinement_63b3e3d8.tar.gz` and `native_matrix_63b3e3d8.tar.gz` retain real
bounded solver outputs, full waveforms and mesh reports/digest records for all requested modes
and estimator restrictions. The report fixtures deliberately use synthetic prior
selection/eligibility values; they do not establish optimizer success. The
withheld-current outputs are fine-model post-run diagnostics, distinct from the
original L0/common-validator paired diagnostics retained in the main capture.

`paired_build_1c6784e7.tar.gz` retains actual release-build receipts for both
executables, with compiler logs and source/process records. Compiled binary
bytes and derived mesh bytes have been removed. The paired replay checks
historical record consistency and reports `binary_bytes_verified: false`;
production build verification still refuses the unhydrated archive. Native
result tests use regenerated meshes that match the original size and digest.
`test_archive_hygiene.py` prevents payloads returning under renamed members.
The same-length schedule mutations now exercise public finalization with a
recomputed execution digest; the identity gate remains independent of row count.
`environment_318c23ad.tar.gz` retains actual environment record/reverification
against a synthetic prepared bundle. `runner_native_1885192d.tar.gz` retains one
real reference geometry query and one primary check through the owned runner
primitive, with all 60 samples accepted by the scientific adapter. It is a bounded
wiring smoke, **not** a full prepared-cohort run. Full public run/finalize behavior
is tested using controlled real processes with explicitly synthetic outputs.

The native runner smoke initially refused an older environment digest. Only the
full environment hash differed; all other recorded source/build/runtime fields
matched. The refusal is retained, and a new explicit local environment record was
made for that smoke. It supports no timing or performance comparison.

## Trust boundaries and operational handoff

Independently retained digests are required at every artifact boundary. They are
local witnessed receipts, not signatures or independent review approval. Offline
process replay establishes consistency with retained observations, not current
absence. Runtime execution remains tied to the original pinned source checkout
because the binaries resolve reference/mesher paths through `CARGO_MANIFEST_DIR`.
Keep `/Volumes/Stripe/geode-fem/codex-issue-1203-executor-build` at `1c6784e7` while
its existing binaries are used; a different host needs its own recorded builds.

The commands and exact arguments for capture, selection, preparation, paired
builds, environment recording, run/finalize and reporting are in `README.md`.
No main final checks may be interpreted until independent review, a verified
closed main capture, full selection, execution/finalization and scientific
reporting are complete. Failures qualify the corresponding result; they never
cause replacement, budget changes or retroactive attainment credit. Full-study
statistical analysis, paper figures and independent manuscript audit remain
publication requirements outside this implementation PR.
