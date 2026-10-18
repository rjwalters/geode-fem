# Raw-trace adapter contract for #1203

This is the implementation handoff from a read-only inspection of #1206 at
`89d0bf9e`. `adapter-source-contract.json` binds the inspected files. It is
not a complete adapter or a main-study result. The generation reader implements
the byte-binding and per-attempt identity layer, with the limits below. Reconcile the map against
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
credit predicate. Starts, restoration and optimizer constraints keep the exact admissibility
rule. The raw `geometry.buildable` flag describes successful construction
and physical guards, not the normalized five-percent margin; it must not be
redefined as `h >= 0`.
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
credit. The tests below cover the implemented layers; complete integration and design/runtime checks remain pending.

The pure helper `geometry_evidence.verify_geometry` now checks the normalized
margin/declared-verdict portion of this map. Its tests include the explicit
`h = -5e-16` regression requested in Squad message 911. The file, run, mesh
and full-geometry provenance layers above remain unimplemented.


`trace_archive.load_generation` now verifies and loads one directory's frozen
inputs, journal, bound terminal traces and optional terminal manifest. It retains
unfinalized/missing traces explicitly and never establishes whole-study closure.
Its local read lock and second hash pass do not prove that a copied remote
experiment is inactive. Recovery-chain reconciliation and all scientific
validation checks above must precede selection. See `trace-archive-checks.json`
for the scoped tests and real writer/synthetic evaluator schema probe.

The recorded scientific-decision layer is now `validation_evidence.py`.
`verify_validation` checks margin normalization, registered strictness/scales,
mesh policy and repair order, aggregate arithmetic, charged completion time and
the target/mean rules. `adapt_validation` converts that evidence to selector
records using caller-supplied reconciled context and full bound design identity.
It does not itself authenticate that context, reconstruct geometry, read mesh
bytes, or resolve the recovery registry. The original `attained` flag is a
numerical decision independent of `credited`; post-budget rows must retain that
flag while receiving no target credit. Eight-ulp arithmetic checks do not soften
any eligibility inequality. See `validation-evidence-checks.json` for exact
source and test/probe scope.


The recovery layer is now `recovery_evidence.reconcile`. It consumes the original
session and an explicitly supplied registry capture, verifies original inputs and
each reservation/link/seal, and recomputes actions with the exact pinned production
policy. It retains all attempts, selects the first clean completion irrespective of
scientific success, carries allowances across generations, and verifies diagnostic
snapshot identity. Original source paths survive copying; external evidence is
hash-checked before in-memory path remapping. No registry constructor or dispatcher
runs while analyzing evidence.

This resolves dispositions within a supplied capture, not the completeness of that
capture or scientific eligibility. `all_registered_jobs_resolved` requires 100
selected completions, no unresolved/pending actions, all recovery seals and a complete
final manifest. `can_select` and `can_launch` remain false. A stopped remote capture,
reviewed external cause, global source/runtime/anchor bindings, full physical design
reconstruction and actual mesh bytes are still caller obligations. The source pin
must include `recovery_policy.py`, `session_journal.py` and `process_identity.py`;
verified source bytes execute directly rather than trusting cached bytecode.
See `recovery-evidence-checks.json` for synthetic tests, mutations and exact policy
agreement with the production Registry/inventory schema probe.
