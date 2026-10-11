# v8 evidence and figure provenance

The inherited manuscript and evidence baseline is v7 at
`a50a446a955c02a8e361c8482d765fb48f254fc6` (PR #1040), including main at
`f4f24a2a`. The detailed historical evidence map remains in
[the v7 provenance](../transmon-benchmark.7/provenance.md). No solver run was
performed for this figure revision and no previous review status transfers.

The new Figure 5 and refined-optimization subsection consume experimental
artifacts from PR #1058 at `1b445770abd268e44c542e9363047583c8b6e9b5`.
Independent review of that experiment is pending. Its code baseline is
`12b91c0d`; this paper branch does not import the solver changes.

| Local snapshot | Source at the pinned commit |
|---|---|
| `figures/src/data/refined_results.json` | `benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/results.json` |
| `figures/src/data/refined_design.json` | `benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/design.json` |
| `figures/src/data/refined_validation.json` | `benchmarks/transmon_diffopt/refined_optimization/validation_summary.json` |
| `figures/src/data/optimized_island.json` | Derived from the experiment's exported `p2_cycles10/final_coarse.msh`: the 139 type-2 triangles in physical group 102, coordinates converted from metres to micrometres. The export is a local experimental artifact, not a committed mesh; its SHA-256 is recorded in this snapshot. No smoothing or fitted shape is used. |

The three source JSON files are exact snapshots. `data/provenance.json` records
their hashes, the derived-outline hash, source commit, and hashes of the five
committed repository inputs used by the other plots. Initial conductor outlines
come from `crates/geode-core/tests/fixtures/transmon_smoke.msh`.

The local result is three accepted gradient steps from 78.387918 to 89.900518 fF,
with a 89.900294 fF Palace check and 89.662755 fF finer-mesh check. These compare
a scalar-epsilon electrostatic model on fixed polygonal geometry. Mesh refinement
is outside the differentiated parameterization. They do not close the pending
continuum-convergence study (#1051), two-target study (#1037), tensor shape
optimization, CAD remeshing, or full-wave inverse design. A recorded P3 attempt
was killed incomplete and is not positive evidence. Exploratory P1 runs informed
the choices; the manuscript states that scope.

Figures 1–4 and 6 read committed mesh/TOML artifacts directly. Figure 7 preserves
the old analytic and one-parameter data in the appendix; it is not the trajectory
of the refined optimization. Both verification scripts check author-side
transcription and regeneration, not independent scientific correctness.

No bibliography entries or citation keys were added. Existing operator inputs,
active pending markers, and inherited citation-support limitations remain.
