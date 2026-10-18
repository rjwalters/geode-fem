# Main motor study: prospective runbook

Status: operations draft, 2026-10-18. **Not a launch authorization or a
tested command recipe.** Bind the commands to the reviewed merges of
[#1201](https://github.com/rjwalters/geode-fem/issues/1201) and
[#1202](https://github.com/rjwalters/geode-fem/issues/1202) before use.
The operator's host choice is pending. This file is outside the audited v6
manuscript and contains no main-study results.

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
before measuring host idleness or calibration.

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
actual environment, not just intended settings. Keep the host awake
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

## 3. Bind and execute the command sequence

The following are the **observed #1201 draft interfaces**, not final
main-study commands. Run from the eventual pinned checkout. Replace each
angle-bracket argument with its recorded absolute path/value. Do not run
these templates literally or against the present three-method draft.

1. Inspect `pyrun.sh`, then use its hash-locked Python environment for all
   harness commands. An environment path supplied through
   `MOTOR_HARNESS_VENV` must be private to this study.
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
3. Export and inspect the pending #1202 queue. It must have 100 unique
   logical jobs: five methods, two spaces, five starts, two repeats.
   Within each space/start s (theta0 = 0 through lhs4 = 4), rotate ABCDE
   left by s for repeat 1 and reverse that exact order for repeat 2.
   Archive planned order and actual dispatch/start order separately;
   concurrency and deferred jobs can make them differ.
4. Create a new, previously nonexistent output directory. The current
   session entry point combines gated calibration and scheduling:

   ```text
   benchmarks/motor_optimization/harness/pyrun.sh benchmarks/motor_optimization/harness/session.py --main --procs <N> --binary <evaluator> --out <new-session-dir> --host-gate on --max-wait 1800 --max-deferrals 3 --load-interval 5
   ```

   **Pending binding:** verify all flags against merged `--help`, confirm
   five-method dispatch, and insert the exact queue-export and recovery
   commands here. Do not invent a `--resume` flag: the inspected #1201
   draft has none and can overwrite an existing output directory.

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
coordinates/charges. Do not rerun ordinary scientific failures merely to
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
Preserve all original files. Resume only unresolved jobs in a new recovery
directory linked to the original session, under the same pinned settings
and verified host condition. Completed eligible jobs must not be repeated
or overwritten. An interrupted attempt needs an explicit predeclared
disposition before any whole-run retry; retain both records and costs.

If the host, source, meshes, settings or calibration must change, stop
combining records and obtain a documented protocol decision. A command
template alone is not proof that recovery is implemented or reviewed.

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
