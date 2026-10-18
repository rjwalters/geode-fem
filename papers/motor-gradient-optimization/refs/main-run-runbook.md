# Main motor study: prospective runbook

Status: operations draft, 2026-10-18. **Not a launch authorization or a
tested native command recipe.** Bind the commands to the reviewed merges of
[#1201](https://github.com/rjwalters/geode-fem/issues/1201) and
[#1202](https://github.com/rjwalters/geode-fem/issues/1202) before use.
The private interface observations below are bound to `d1bae200`, with the
queue exporter added at `d9a72442`. Dependency #1201 merged through reviewed
PR #1204 at `5c998bda`. At `bc1527e8`, post-rebase verification passed the
222-test Python suite, 98 safeguard mutations, native starts/gradient checks
and both-space L0/L2 smokes. The subsequently adopted cross-platform anchor
gate is implemented at `d54c66b2`, with a test-only follow-up at `728b38b2`;
its native verification is in progress before the #1202 PR. Claude reports
operator approval for the cloud run with
a $50 total cap (Squad message 862, 2026-10-18). Main execution remains
conditional on reviewed #1202 integration and host checks below. This file
is outside the audited v6 manuscript and contains no main-study results.

The governing protocol is #1185 amendments 7 and 8, including the adopted
corrections and final-selection/start clarifications linked in
[EXPERIMENTS.md](../EXPERIMENTS.md). A discrepancy between this draft and
that protocol must be resolved before execution, not by adjusting a run
after inspecting its outcome.

## 1. Freeze the executable study

Record the merged source commit, clean checkout status, evaluator binary
SHA-256, build command and log, Rust/compiler versions, Python version and
package-lock SHA-256. Use a private Cargo target directory under the
repository's build-isolation instructions. Build and install dependencies
before measuring host idleness or calibration. The private candidate's
`build_provenance.py` records a Cargo invocation, checks clean source/build-environment
stability, and retains a hash-bound executable copy. Main execution requires
its receipt; a binary hash alone is insufficient. A changed source commit
requires a new build record. Cached Cargo artifacts remain explicitly labelled.

Freeze the anchor, L0/L1 training meshes and L2 validation mesh with their
hashes; the geometry inventory/scales; the five start vectors and their
IEEE-754 bit patterns; solver/material/current settings; optimizer settings;
and all method specifications. A/B/C train at L1, D/E at L0; every method
uses the same registered L2/h93/g20 validation and anchor. Training is
h45/g20 at fractions 0, 0.5 and 1 of the fixed current. The budget is
120 common validation-time units, with a completed-validation checkpoint
at 60 units.

Require the reviewed mixed L0-training/L2-validation smoke and gradient
checks from #1202. The all-L0 #1201 smoke cannot establish that wiring.
Confirm that CPU/RSS accounting and post-run diagnostics are present.

## 2. Prepare the selected host

Record hostname, UTC time, OS/kernel, CPU model, logical/physical core
counts where available, RAM, process count N, thread limits, affinity
support/settings, power settings, and free archive space. Preserve the
actual environment, not just intended settings. Include the explicit NumPy/SciPy
and mesher BLAS/LAPACK configuration, package payload hashes and backend/library
override environment. Accelerate can report version `unknown`; retain its named
backend and OS version as well. Fix these identities across the session and
recovery; matching package version strings alone is insufficient. Keep the host awake
through the session and record interruptions or power-state changes.

The host must otherwise be idle: no concurrent FEM studies, Rust builds,
CI/Loom worker builds, benchmark sessions or other sustained compute jobs.
Coordinate with their owners before changing their activity. Finish setup
and allow load to settle. Verify the child processes use the registered
single-thread settings; N is fixed for calibration and optimization.

The pre-run gate requires one-minute load average <= N + 1. During the
in-run interval, load > 1.5 N continuously for more than 60 seconds marks
that attempt contaminated. Preserve timestamped samples, gate waits,
deferrals and phase boundaries. Post-run diagnostic load is separate.
These thresholds do not replace checking for competing work.

### Cloud probe and host preparation

Squad message 862 (Claude, 2026-10-18) reports operator approval of up to
$50 total and initiation of a probe on the on-demand x86 `c7a.4xlarge`
named `exp2main`. Claude selected N = 15 after the probe (message 872).
The reported setup uses main `5c998bda`, rustc 1.98.1 and the gmsh 4.15.2
SDK. Verify the actual machine and pinned dependency installation in the
probe records. Earlier price and duration estimates in message 842 remain
planning estimates, not measured benchmark costs.

The probe ran no optimizer jobs. Retained records are in
`/Volumes/Stripe/scratch-codex-review/exp2-cloud-probe/`.
`probe_record.json` has SHA-256
`fce47740166679af4f651e712b1ccdd242530b8f124cc9d2a4226433b220a9bf`.
[Independent arithmetic checks](cloud-probe-arithmetic-audit-20261018.json) of its raw anchor/calibration records reproduce
relative F0 error `2.4043e-13`, relative full-current mean error `7.3972e-15`,
and worst per-angle torque difference `1.0925e-13 N m` in both spaces.
Newton iteration counts match. The probe serialized an additional
`mesh_repair: false` field absent from the historical anchor; shared physical
settings and the validation mesh hash match. Bitwise and four-ulp
cross-platform agreement flags are both false and remain in the records.

The raw medians are 49.4937 s from three single-process timings, 69.4025 s
from 45 timings at N = 15, and 64.6915 s from 36 timings at N = 12. These
are probe observations, not the final main calibration or optimizer results.
Claude reports the instance stopped with its build and meshes retained.
Main jobs await reviewed #1202 merge, a fresh recorded build and host checks.

The [adopted main acceptance rule](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6252347196)
requires relative F0 and mean differences <= `1e-9`, and absolute torque
differences <= `1e-9 N m` at every current and angle, before calibration.
The earlier `1e-6` was only the probe's stop threshold. Keep four-ulp
anchor-file parsing, exact input/physics hashes and same-host determinism;
Newton counts are recorded without an equality requirement. Targets, delta-F
and the mean floor always come from the committed anchor, never the recomputation.

Complete these steps before main calibration:

- Copy the frozen L1/L2 theta0 meshes from the Mac and verify their hashes;
  do not replace them with meshes regenerated by Linux gmsh. Preserve the
  remaining registered inputs and hashes as required above.
- Rerun theta0 validation on the selected host and compare its outputs
  with the frozen `anchor.json` under the reviewed acceptance criteria.
  Investigate differences before calibration; do not silently substitute
  a new anchor or invent a tolerance after seeing the outputs.
- Record the actual OpenBLAS configuration expected on that host, package
  payloads, OS and library overrides. Treat the Mac's Accelerate smoke as
  separate evidence; redo the required host-specific checks.
- Use `AWS_PROFILE=geode-fem-bench` and resource tag `project=geode-fem`;
  disable automatic stop actions throughout active setup, calibration,
  optimization and diagnostics, or establish an explicit workload-aware
  policy that cannot stop those phases. The probe's below-5%-CPU-for-30-minutes
  alarm can mistake serial work for idleness; raising that threshold makes
  the risk worse. Re-enable idle stop only after confirming work is finished
  and records are preserved. Record the policy and archive any stop as an interruption, with recovery eligibility
  determined by the adopted policy.
- Copy raw records and hash inventories back to the Mac the same day and
  verify the copy before terminating storage or compute resources.

The selected route excludes Spot: a replacement host would violate the
fixed-host recovery bindings and require a protocol decision. Retain the
probe's costs and calibration separately from the main session; probe timings
do not replace the registered main calibration after the final reviewed build.

## 3. Bind and execute the command sequence

The following are **observed private #1202 interfaces at d1bae200**, not
reviewed native main-study commands. Run from the eventual pinned checkout.
Replace each angle-bracket argument with its recorded absolute path/value.
Do not run these templates literally or before the required reviewed merges.

1. Inspect `pyrun.sh`, then use its hash-locked Python environment for all
   harness commands. An environment path supplied through
   `MOTOR_HARNESS_VENV` must be private to this study. Record the evaluator
   build in a new directory under the private target:

   ```text
   benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/build_provenance.py --out <new-build-record-dir>
   ```

   The recorded command uses `cargo build --locked --release -p geode-core
   --example icem_harness_eval`. Use the retained `icem_harness_eval` copy
   and `build-receipt.json` from that directory. The recorder has controlled
   process tests and a successful native invocation at `2c273781`. That
   receipt does not bind subsequent source commits: record a new build on
   the final reviewed checkout before use.
   `pyrun.sh` requests CPython 3.12, not the exact 3.12.13 patch. The cloud
   probe used 3.12.3 and uv 0.13.1; record the actual interpreter and package
   contents and hold them fixed within the main session and recovery.
2. Reproduce the start preflight into a separate file; do not overwrite
   the committed starts:

   ```text
   benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/make_starts.py --binary <evaluator> --out <preflight-dir>/starts-recheck.json
   ```

   Compare exact chosen vector bits, selected/rejected indices and guard
   decisions to the frozen record. Timing fields may differ. Every start
   must pass buildability, h >= 0 and moved-mesh quality without remeshing
   in both spaces at L0 and L1. Record preflight cost outside run budgets.
   A mismatch needs investigation before launch; never silently replace
   starts based on optimizer outcomes.
   The launcher then runs the full-waveform anchor preflight automatically,
   before calibration. A standalone check on the final host is:

   ```text
   benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/anchor_preflight.py --binary <evaluator> --out <preflight-dir>/anchor-recheck.json
   ```

   Retain its raw A/B replies, per-sample comparisons, hashes and resource
   costs. Its recomputed values verify host suitability and are not a new
   anchor or calibration unit. Apply the adopted `1e-9` limits above.
3. Export and inspect the #1202 queue. The private exporter is implemented
   at `d9a72442`; it uses the same method registry and queue constructor as
   the launcher, and runs no FEM or calibration:

   ```text
   benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/export_queue.py --out <existing-archive-dir>/planned-queue.json
   ```

   Use a new filename; existing output is refused. Retain its printed file
   hash and inspect the recorded source commit, file hashes and clean-tree
   flag. Dirty development exports are labelled and cannot establish main
   readiness. Re-export from the reviewed merged checkout before launch.
   It must have 100 unique logical jobs: five methods, two spaces, five starts, two repeats.
   Within each space/start s (theta0 = 0 through lhs4 = 4), rotate ABCDE
   left by s for repeat 1 and reverse that exact order for repeat 2.
   Compare the full exported `jobs` array with the session's frozen
   `queue.json`. Archive planned order and actual dispatch/start order separately;
   concurrency and deferred jobs can make them differ.
4. Create a new, previously nonexistent output directory. The current
   session entry point combines gated calibration and scheduling:

   ```text
   benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/session.py --main --procs <N> --binary <build-record-dir>/icem_harness_eval --build-receipt <build-record-dir>/build-receipt.json --out <new-session-dir> --host-gate on --max-wait 1800 --max-deferrals 3 --load-interval 5
   ```

   **Pending binding:** verify all flags against merged `--help`, confirm
   five-method dispatch, and archive the concrete queue-export invocation.
   `main_schedule.registered_jobs()` supplies the shared queue construction.
   The private session launcher refuses
   an existing output directory. Recovery uses the separate command below;
   it is not optimizer-state resume.

Calibration uses N simultaneous processes with three cold theta0
validations each; freeze the median of all 3N timings as the shared unit.
Keep the single-process diagnostic separately. Measure L0/L1 training
normalizations/weights without introducing separate validation-time units.
Require uncontaminated calibration before main scheduling. If calibration
is contaminated, preserve it and establish a clean calibration before
launch; merely marking a whole session invalid at its end is insufficient
for this operational preflight. Verify this sequencing in #1202.

## 4. Monitor and retain every attempt

A gate timeout is a deferred job, with no optimizer process launched.
Requeue at the back within the recorded deferral cap. If the cap is reached,
retain an explicit outstanding job; a scheduler exit is not evidence of a
complete 100-job study. Record the exact cap semantics from the merged code.

Contaminated attempts are preserved and rerun at the session's end with
unique attempt IDs. Keep the original logical job/start/repeat identity.
Select the eligible rerun by a frozen provenance rule, never by better F
or faster time. A rerun that is contaminated or deferred remains unresolved;
the inspected draft schedules only one automatic contamination rerun.
Do not relabel it clean or silently choose a favorable attempt.

Check for parseable traces, progress, disk space, evaluator failures and
load samples without adding competing compute. Preserve budget stops,
restoration failures, infrastructure failures, refusals and aborted request
coordinates/charges. Preserve raw solver return status/message/counts and
whole-run cap flags; keep local C subproblem caps separate. An unsuccessful
solver return is not convergence. Do not rerun ordinary scientific failures merely to
improve the result. Keep in-run wall/CPU accounting distinct from setup and
post-run diagnostics; RSS high-water marks are not additive memory peaks.

## 5. Interruption and recovery

There is no demonstrated optimizer-state checkpoint/resume in the inspected
draft. The 60-FE result is an analysis checkpoint, not saved solver state.
Do not concatenate a restarted optimizer trajectory with an interrupted
one, grant it a fresh budget while presenting it as continuous, or mix a
new time-unit calibration into old records without explicit provenance.

Before launch, bind and test a durable session-level recovery procedure in
#1202: save immutable session/queue/calibration metadata and append attempt
status/load evidence during execution, not just at normal session exit.
On interruption, inventory completed, running/partial and never-started
jobs; verify no orphan evaluator is still executing before any replacement.
Preserve all original files and the first clean completed attempt, including
scientific failures and censored results. The [adopted interruption policy](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6249073475)
allows at most one whole-run replacement per logical job for a **verified
external interruption**, after proof that old processes have stopped. Unknown
crashes stay unresolved; solver errors, mesh infeasibility, safeguard stops and
target misses are not retry permission. Recovery never resets the separate
single contamination-rerun allowance.

Recovered never-started/deferred jobs and verified external-interruption
replacements run after the remaining registered queue, in their original
relative order. Use a new linked directory and distinct attempt IDs. A
replacement receives the full registered budget and is labelled a new attempt;
never concatenate trajectories. Match source, environment, binary/input hashes,
settings, host and worker count. Reuse the original recorded calibration unit
without remeasurement; changed host or unusable unit needs a new protocol
decision. Apply the host gate to replacements. Preserve original costs,
explicitly marking incomplete measurements. Report every replacement with its
reason/evidence/partial costs and the count of affected jobs.

If optimization completed but its post-run diagnostic was interrupted, repair
only the diagnostic with separate provenance and cost; never repeat optimization.
The private #1202 branch now includes the durable journal, stopped-result
snapshots, owned-process cleanup/receipts, orphan verification, fixed recovery
registry and diagnostic-only workers. Controlled execution tests cover these
paths. Bind and review them again after dependency integration; this is not
main-run approval.

```text
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/recovery_inventory.py --source <original-session-dir> --out <new-inventory.json>
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/process_verification.py --source <original-session-dir> --out <new-process-report.json>
benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/recovery.py --source <original-session-dir> --main [--external-events <reviewed-events.json>]
```

The inventory and process observer do not launch work. The linked launcher
holds the fixed registry lock, reads every generation and rechecks source,
runtime (including BLAS/LAPACK), input and process evidence before dispatch.
It has no overrides for N, unit, budget, numerical settings or output directory.
A failed verification requires disposition, not a fresh directory that resets
allowances. Reviewed external-event attestations identify the attempt, external
cause, reviewer/reference and hash-bound evidence; a signal alone is insufficient.
Diagnostic repairs get separate attempts and costs, with no optimizer relaunch.
Every generation retains an immutable plan and sealed summary. Preparation and
verification elapsed times may overlap worker time and must not be added twice.

## 6. Archive, review and hand off

Freeze an attempt manifest mapping each logical job to every attempt,
eligibility decision and raw trace hash. Require all 100 logical jobs to
have an explicit terminal disposition; disclose unresolved/deferred jobs
as incomplete coverage. Keep both repeats and all failures. Generate
60/120-FE tables and figures from the same hash-verified records.

Archive raw traces, calibration, starts/preflight, queue/dispatch history,
load samples, stdout/stderr, environment and source/binary hashes, geometry
refusals/repair attempts and separate diagnostic records. Hash large `.msh`
files and retain them in a retrievable external archive with path/size/hash
inventory; a hash alone does not preserve a mesh. Verify retrieval and
regeneration instructions. Add a narrowly scoped ignore rule for generated
mesh directories in the integration worktree; verify it with
`git check-ignore`. Do not assume existing global ignores cover them.
Logs are also commonly ignored: stage intended evidence explicitly.

Commit the compact evidence, inventory, reproducible analysis and README
on a dedicated results branch after checking paths and contents. Preserve
the original raw files and log every derived transformation. Review the
results PR before merging through the repository's approved merge script.
No public upload or submission is implied by this runbook.

Hand #1203 the frozen attempt manifest, exact design bits and physical
settings, common anchor, eligibility/contamination decisions and source
hashes. Its reviewed adapter selects up to ten feasible representatives
and thirty earliest repeat-1 target crossings, with explicit missing cells
and exact-geometry/settings deduplication. Retain earlier eligible
validations from subsequently failed runs. Run the registered refinement
and withheld-current checks, preserve failures without substituting another
candidate, and keep all costs separate from time-to-target. Only then
integrate measured results into a new manuscript and request fresh reviews
and audit.
