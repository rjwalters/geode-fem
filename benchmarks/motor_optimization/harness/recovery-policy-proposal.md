# Adopted interrupted-session disposition policy (#1202)

Adopted before any main run in Squad review 28 and
[#1185 comment 6249073475](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6249073475).
The filename retains the proposal history. Policy adoption is not implementation
approval: the current inventory does not launch anything.

- Preserve the first uncontaminated, completed attempt for a logical job, including
  ordinary scientific failures or censored results. Never select by objective,
  target time, or optimizer success. Preserve all later/contaminated records too.
- Never-started and gate-deferred jobs may run in a new linked recovery directory
  under the original source, binary, input hashes, settings, worker count, host and
  calibration unit. Preserve the original session. Do not concatenate trajectories.
- Recovered never-started/deferred jobs and external-interruption replacements
  run after the remaining registered queue, in their original relative order.
  Recovery never reorders the primary queue.
- A completed contaminated attempt gets only the existing one contamination rerun;
  recovery cannot reset that allowance or turn repeated contamination into clean
  data. Preserve the original and the rerun costs.
- External-interruption rule: permit at most one whole-run replacement
  per logical job for a verified external interruption (e.g. operator stop or host
  shutdown), only after proving the old launcher, run and evaluator processes have
  stopped. Give the replacement a distinct attempt ID and a full registered budget;
  disclose that it is a new attempt, not a continued trajectory. Retain original
  partial results and measured costs, explicitly incomplete where needed.
- Do not treat a solver error, mesh infeasibility, safeguard stop, target miss,
  unknown crash, or changed artifact as a verified external interruption. Those
  outcomes are preserved; unknown cases remain unresolved for a documented decision.
- Reject recovery if source/environment/input hashes or the host/N/calibration unit
  differ. Reuse the session's recorded unit; never remeasure it. A changed host or
  unusable original unit requires a new protocol decision, not automatic
  calibration combined with the old records. Reapply the host gate to every replacement run.
- A successful optimizer trace with an interrupted post-run diagnostic must not
  repeat optimization. Preserve the stopped-run result and mark the diagnostic
  unfinished; any diagnostic-only repair has separate cost/provenance.

The paper and summary must list every replacement, its reason and evidence, and
the original partial costs, and count the affected logical jobs. Unverified
interruptions remain unresolved with no automatic rerun.

Pending implementation: process-identity/orphan verification (especially after a
forced exit), linked recovery execution, diagnostic-only recovery, and execution
tests of this adopted policy. Cooperative run/session cancellation, stopped-result
snapshots and sole-reaper launcher receipts now have controlled process tests;
these do not by themselves authorize a replacement. `recovery_inventory.py` currently
separates completed, never-started/deferred, contaminated and interrupted cases,
checks bound inputs/traces, reports active ownership/torn journal tails, and refuses
to treat an interrupted attempt as an automatically launchable job.
