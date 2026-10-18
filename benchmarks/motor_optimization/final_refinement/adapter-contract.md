# Raw-trace adapter contract for #1203

This is the implementation handoff from a read-only inspection of #1206 at
`d5634324`. `adapter-source-contract.json` binds the inspected files. It is
not an implemented adapter or a main-study result. Reconcile the map against
the reviewed merge before integrating; do not import or alter the live run.

## Resolve the study before selecting designs

1. Read the original session, frozen queue, bindings, journal and all linked
   recovery generations. Verify each frozen file and terminal trace digest.
   The original queue must match the registered 100 jobs exactly, including
   method letters, start indices and timing repeats. Never infer the cohort
   from whichever trace files happen to be present.
2. Use the semantics of `recovery_inventory.inventory` and
   `recovery_policy.plan` to reconcile attempts: the first clean completed
   attempt wins, including scientific failures and budget-censored outcomes.
   Preserve all rejected/unfinished/contaminated attempts and their costs.
   A diagnostic-only repair does not replace or add an optimizer attempt.
3. Freeze the final selection only from a closed, hash-stable study snapshot.
   Active owners, torn journals, unresolved interruptions and missing terminal
   host evidence are explicit unresolved states. Report them; do not silently
   turn them into a failed optimization or a new replacement permission.
   A live progress report cannot be labelled the final frozen cohort.
4. Do not instantiate `recovery_registry.Registry` simply to analyze an archive:
   its constructor can create a registry and append events. The adapter needs
   read-only history loading with checked path mapping for copied archives.
   Frozen absolute source paths and their hashes remain evidence; an explicit
   source-to-archive mapping must not edit the original records.

## Map validation records

| Selector field | Raw evidence / check |
| --- | --- |
| `run_id` | A stable original-session/generation/attempt identity, bound to the terminal trace digest; not just the logical job name |
| `source_sha256` | Recomputed complete trace-file SHA-256, matched to `attempt_finished.result.trace_sha256` |
| `method`, `space`, `start_index`, `repeat` | Frozen queue job plus matching trace metadata and method registry; never parse these from filenames |
| `ordinal` | `validations[].index`; reject duplicates and inconsistent ordering |
| `elapsed_seconds` | `validations[].seconds`, stamped after validation completes; cross-check `wall_fe * unit_seconds` with disclosed floating-point tolerance |
| `budget_seconds` | Frozen `budget_fe * unit_seconds`; all five methods share that same unit |
| `f_val`, `mean_full` | `F_val`, `mean_full`, finite and consistent with recorded means/stds/current identities |
| `eligible_run` | The reconciled selected completion, independently of solver success or target attainment |
| `host_contaminated` | Explicit terminal launcher evidence, not inferred from a trace load snapshot |
| `completed` | A complete `status=ok` validation in the chosen attempt; a `validation_unavailable` event cannot stand in for it |
| `geometry_admissible` | Recompute credit admissibility from complete finite geometry margins and buildability, then cross-check `admissible` |
| `discrete_pass` | Every registered discrete guard is present and passes with its stated strictness |
| `mesh_pass` | Retained validation `mesh.quality` and mesh policy, including accepted fresh-mesh attempt history and actual mesh digest |
| `stop_reason` | Original trace reason; later scientific failure does not erase earlier valid records |

The adopted credit and best-feasible rule uses every `h >= -1e-12`.
`h_nonnegative` records the stricter `h >= 0` rule and is **not** the selector's
credit predicate. Buildability, starts and restoration keep their exact rule.
Check the complete registered guard name set; `all([])` is not evidence.
Missing margins, NaNs, unknown mesh kinds, absent quality or missing discrete
guards must not yield an eligible design.

Recompute target attainment from the canonical anchor with
`F + max(0.1*F, 2.5e-7)` and the `1.0025*mean0` floor. Cross-check the driver's
`credited`, `mean_ok`, `admissible` and `attained` flags; never rely on them
alone. Budget credit is based on completed validation time, not the time its
request started or total optimizer/diagnostic duration.

Preserve every validation from every attempt, with explicit exclusion reasons.
A failed run's earlier eligible validations remain available. The selector
then chooses representatives across both repeats and earliest crossings from
repeat 1 only, using its frozen tie-break. A refinement failure cannot cause
selection of the next-best candidate.

## Complete design and refinement provenance

`validations[].x` is the numerical coordinate vector. Bind its binary64 values
to the registered coordinate order, fixed geometry constants, zero/nonzero
control offsets, source and material/current configuration. Deduplication must
include every setting that changes the final check, including the actual motion
convention; optimizer identity or L0/L1 training resolution alone is not a
physical difference. Do not round coordinates for identity or infer equivalence
from equal objective values.

Current `op_validate`/trace records retain means, standard deviations, mesh
provenance and objective, but not complete torque waveforms. They can establish
selection and original credit; they cannot satisfy the final comparison API's
raw-waveform contract. Run a fresh primary comparison unless a separately
retained record proves exact waveform/settings/source equivalence. Collect all
L3/g40/tight/shifted/withheld checks with separate post-run cost receipts and
matching-setting theta0 references. Conforming eligibility still requires
actual zero stator offsets, a fresh mesh and supported rotations.

The adapter must test corrupted hashes, missing jobs, unfinalized traces,
changed units/anchors/settings, ambiguous replacements, ignored contamination,
post-budget records, tiny negative admissibility margins, empty guard lists,
coordinate rounding and attempts to substitute diagnostic results for primary
credit. These tests are pending implementation, not discharged by this map.
