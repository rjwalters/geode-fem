# Frozen-cohort final refinement (#1203)

This pipeline authenticates a closed main study, selects the prescribed cohort,
prepares exact requests, records the executable/runtime inputs, runs the checks,
and produces a scientific summary from finalized execution evidence. The
implementation is ready for independent review. The main study has not yet been
captured as closed or refined; no optimization-performance result is claimed.

The protocol is specified by [#1203](https://github.com/rjwalters/geode-fem/issues/1203)
and the adopted [selection addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247383821),
[eligibility clarification](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6247421487)
and [refinement addendum](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6248492318).
[EVIDENCE.md](EVIDENCE.md) maps each acceptance requirement to code, tests, mutation
checks and retained native evidence. Historical `*-checks.json` test counts apply
to their recorded source revisions; they are not the current suite total.

## Tests and evidence

From a full-history repository checkout:

```sh
python3 -I -m unittest discover \
  -s benchmarks/motor_optimization/final_refinement -p 'test_*.py' -v
```

The stdlib suite uses retained native outputs and controlled Python child
processes. It does not compile or execute archived binaries. The dedicated CI job
uses Python 3.12 and full Git history. The Rust `motor_final_refinement` integration
target is covered by the existing core integration workflow.

Archived native checks cover primary, L3, g40, tight Newton tolerance, independent
shifted solves, withheld currents and conforming-estimator restrictions. The
paired release-build archive retains both executables, compiler inputs/logs,
source census and process observations. The bounded runner archive retains one
real geometry query and one primary check with 60 accepted torque samples. These
are wiring/refinement checks; selection fixtures are explicitly synthetic. Full
public run/finalize behavior uses controlled real processes with synthetic outputs.
See the individual evidence records for exact scope, digests and replay commands.

## Frozen selection and check schedule

Selection retains all ten method/space representative cells and thirty repeat-1
crossing cells, including missing outcomes. Every considered validation retains
its source identity and rejection reasons. Eligibility requires a completed
within-budget validation, the registered mean floor, geometry/discrete/mesh
admissibility and an eligible uncontaminated run. Earlier eligible validations of
later-failed runs remain available. Ties use start, repeat, then validation ordinal.
Crossings use repeat 1 only and preserve the registered objective-error margin.

Deduplication uses exact full geometry and physical/numerical settings, preserving
all roles. Each space has a theta0 reference at matching settings; equal physical
coordinates do not conflate different motion maps. Refinement outcomes cannot
change the cohort, replace a failed check or alter original attainment/budgets.

| Check | Spatial level / harmonics | Angles (degrees) | Newton relative tolerance | Current fractions |
| --- | --- | --- | --- | --- |
| Primary comparison | L2 / h93 | 0:1:19 | 1e-8 | 0, 0.5, 1 |
| Spatial refinement | L3 / h93 | 0:1:19 | 1e-8 | 0, 0.5, 1 |
| Angular refinement | L2 / h93 | 0:0.5:19.5 | 1e-8 | 0, 0.5, 1 |
| Tight nonlinear solve | L2 / h93 | 0:1:19 | 1e-12 | 0, 0.5, 1 |
| Independent shifted sweep | L2 / h93 | 0.5:1:19.5 | 1e-8 | 0, 0.5, 1 |
| Withheld-current diagnostic | L2 / h93 | 0:1:19 | 1e-8 | 0.25, 0.75 |
| Conforming outer-band Arkkio | Fresh L2 candidate mesh | Supported 0:1:19 rotations | 1e-8 | 0, 0.5, 1 |

Each current sweep starts cold, with continuation between successive angles.
Conforming evaluation requires exactly zero stator offsets and supported rotations;
otherwise the result is explicitly unavailable/model-dependent. g40 subsampling
cannot substitute for independent shifted solves. Withheld-current sums are not
the registered objective. Their fine-model diagnostics remain distinct from the
original L0/common-validator paired diagnostics retained in the source capture.

## Operational sequence

Use reviewed full Git commit IDs and independently retain every returned digest.
Commands create new outputs; they never resume or overwrite earlier evidence.
Keep input bundles, build artifacts, execution directories and output receipts
outside their source checkout. `$TOOLS` below is the absolute path to this
directory in a reviewed checkout, held fixed through execution and reporting.

1. **Capture only a closed study, on its original host.** Run in the original
   isolated numerical environment and working directory. The exporter requires
   the entire registered queue, recoveries and diagnostics to be resolved and
   verifies sources, inputs, runtime, build receipts and stopped processes before
   and after copying. A live main session must not be exported as closed.

   ```sh
   "$CAPTURE_PYTHON" -I -B "$TOOLS/closed_capture.py" \
     --source "$MAIN_SESSION" --repo "$PRODUCER_SRC" \
     --reviewed-commit "$PRODUCER_COMMIT" --out "$NEW_CAPTURE"
   ```

2. **Prepare the exact cohort and all requests.** This reruns closed-capture,
   setup, ledger, scientific eligibility and selection verification. L3 bytes
   come from independently reviewed retained construction evidence whose source
   and initial geometry match the study. The bundle includes selection/rejection
   provenance, reference/start data, L2/L3 meshes, native requests and separate
   geometry queries. Preparation launches no solver.

   ```sh
   python3 -I "$TOOLS/refinement_bundle.py" \
     --capture "$NEW_CAPTURE" --manifest-sha256 "$CAPTURE_SHA256" \
     --git-repo "$GIT_REPO" --reviewed-commit "$PRODUCER_COMMIT" \
     --capture-tool-commit "$CAPTURE_TOOL_COMMIT" \
     --mesh-evidence-commit "$MESH_EVIDENCE_COMMIT" --out "$NEW_BUNDLE"
   ```

   `study_selection.py` also exposes the standalone selection manifest, using
   `--capture`, `--manifest-sha256`, `--git-repo`, `--reviewed-commit`,
   `--capture-tool-commit` and a new `--out` JSON path. Preparation repeats this
   verification rather than trusting an unbound standalone selection file.

3. **Record both executables from one clean pinned executor checkout.** Run from
   `$EXECUTOR_SRC`. The recorder uses its private Cargo target and exact producer
   build functions, retaining compiler inputs, logs, immutable binaries and process
   journals. A partial build is retained as failed evidence.

   ```sh
   python3 -I -B "$TOOLS/refinement_build.py" \
     --repo "$EXECUTOR_SRC" --reviewed-commit "$EXECUTOR_COMMIT" --out "$NEW_BUILDS"
   ```

   Keep that source checkout intact: native reference and mesher paths are compiled
   into the executables. Evidence archives are portable; the executable runtime
   is not made portable by moving an archive. Existing local build evidence at
   `1c6784e7` depends on its preserved `codex-issue-1203-executor-build` checkout.
   A new host requires its own recorded build and environment.

4. **Record the current environment from the executor checkout.** This verifies
   the prepared inputs, both builds, compiled reference/geometry-source paths and
   stopped build processes. It probes the exact isolated native mesher environment
   (NumPy 2.5.4, Gmsh 4.15.2), installed-file and BLAS/LAPACK/library hashes, UV and
   launcher identity. It hashes the full child environment without storing its
   values. `RAYON_NUM_THREADS=1`, `GEODE_NUM_THREADS=1` and `UV_OFFLINE=1` are explicit.
   Missing cached packages cause refusal; prepare pinned packages separately and
   then reverify offline. A changed environment is not silently accepted.

   ```sh
   python3 -I -B "$TOOLS/refinement_environment.py" \
     --bundle-root "$NEW_BUNDLE" --bundle-sha256 "$BUNDLE_SHA256" \
     --build-root "$NEW_BUILDS" --build-sha256 "$BUILD_SHA256" \
     --repo "$EXECUTOR_SRC" --executor-commit "$EXECUTOR_COMMIT" --out "$NEW_ENVIRONMENT"
   ```

5. **Run the frozen schedule from the same checkout/environment.** Every geometry
   query runs first, followed by all planned native checks in deterministic order.
   Separate request directories retain exact commands, stdin, stdout/stderr,
   process resource receipts, native results and all generated mesh/repair files.
   The exec adapter verifies the executable hash and preserves the owned PID/group.
   Refusals/missing outputs stay explicit; interruption reaps the owned child and
   retains a failure marker. There is no automatic retry or candidate replacement.

   ```sh
   python3 -I -B "$TOOLS/refinement_run.py" run \
     --environment-path "$NEW_ENVIRONMENT" --environment-sha256 "$ENV_SHA256" \
     --bundle-root "$NEW_BUNDLE" --bundle-sha256 "$BUNDLE_SHA256" \
     --build-root "$NEW_BUILDS" --build-sha256 "$BUILD_SHA256" \
     --repo "$EXECUTOR_SRC" --executor-commit "$EXECUTOR_COMMIT" --out "$NEW_EXECUTION"
   ```

6. **Finalize after the run process exits.** Use the same arguments as step 5,
   change the action to `finalize` and `--out` to a new external finalization JSON
   path, and add `--execution "$NEW_EXECUTION" --execution-sha256 "$EXECUTION_SHA256"`.
   This verifies current bindings, the complete census/schedule/journal/receipts,
   and stopped runner/child groups. `can_report: true` permits scientific validation
   of the execution evidence; it does not mean every check passed.

7. **Produce the scientific report.** This read-only consumer authenticates all
   supplied receipts, replays retained finalization process evidence, and verifies
   the complete frozen schedule. Native resource PIDs and exit states must match
   their owned-process receipts. Exact requests, settings, geometry, actual mesh
   bytes and waveform aggregates are checked before comparison. Geometry failure,
   abnormal exit and missing outputs remain explicit unavailable comparisons.

   ```sh
   python3 -I -B "$TOOLS/refinement_report.py" \
     --execution-root "$NEW_EXECUTION" --execution-sha256 "$EXECUTION_SHA256" \
     --finalization-path "$FINALIZATION" --finalization-sha256 "$FINALIZATION_SHA256" \
     --bundle-root "$NEW_BUNDLE" --bundle-sha256 "$BUNDLE_SHA256" \
     --build-root "$NEW_BUILDS" --build-sha256 "$BUILD_SHA256" \
     --git-repo "$GIT_REPO" --executor-commit "$EXECUTOR_COMMIT" --out "$NEW_REPORT"
   ```

The summary preserves raw observations, rejection/selection provenance, signed
waveform/mean/std/objective changes, matching-setting target survival, feasible
orderings and separate post-run costs. Withheld sums earn no original target
credit. CPU/RSS scopes are retained individually rather than summed or represented
as a measured process-tree peak. Historical process replay makes no claim about
current process state. These receipts are local witnessed evidence, not signatures
or independent scientific approval; full-study analysis and manuscript review
remain publication requirements.
