# Methods coverage revision: author response

2026-10-18. This responds to the [coverage audit](main-study-methods-protocol-audit.md)
at `06a9a0d9`; it does not overwrite that audit's historical ratings or source
snapshot. The revised [prospective fragment](main-study-methods-draft.tex) has
SHA-256 `ca5bd245b1b6e4d69ad3c04a09eef0738c30ce75d399c2eb39f7ab8ce532ac20`.
This is an author revision, not independent approval. It remains outside the
immutable v6 manuscript and its audit.

## Text changes

| Audit clauses | Revision |
|---|---|
| 8.1, 8.3–8.7 | Names coupling torque; specifies equal optimizer options across resolutions, SLSQP tolerance and COBYQA version/radii/initial-set size; names matched factorial contrasts; distinguishes physical training mean floors; prohibits coarse-model correction/adaptation/validation tuning. |
| 8.14–8.16, inherited I6/addendum | Requires paired low/high-fidelity outputs at every validated D/E point, including failed acceptance; distinguishes D/E selection from C's final incumbent and documents C's interpolation and post-run mean models. |
| 8.G2–8.G3 | Adds scalar/grouped L0 directional central-FD step sweeps at frozen admissible points, retaining failed checks and setup costs, and mixed-resolution validation/anchor verification. Evidence remains pending. |
| 7.1b–7.1f, 7.1h, 7.2a | Explains reference-margin scaling and zero-reference exceptions, safe guard evaluation, value-versus-derivative domains, geometry-only Jacobians, ordinary evaluation of buildable-but-inadmissible points, no outcome-tuning of tau, and no substitution of another design's values. |
| 7.1g, inherited I1/I2, R18 bounds | States fixed size/control constants, Opt120's historical role and the reviewed numerical geometric bounds with their limits. Exact definitions/derivation reference remains pending. |
| 7.2b–7.2g, 7.3a–7.3c | Adds failure termination, mesh restoration predicate, rejected/restored coordinate evidence, C sign/halving/full-rank rules, 5-degree/inversion triggers, exact four repair settings and mesh-infeasible versus infrastructure failure. |
| 7.4b, 8.12–8.13, inherited I4/I5 | Separates pilot/main trajectories, prohibits cross-method cache reuse and selective extra restarts, describes budget process-group abortion, cold validation, training-infeasible incumbent validation, initial-set boundaries and final-point validation. |
| 7.5a–7.5b, S1–S2, 8.10 | Names pilot indices and deterministic seed rules, outcome-independent selection, bitwise pre-outcome freezing, every rejection record, zero-inversion/5-degree moved-mesh eligibility, and geometry-only preflight cost outside budgets. |
| 7.6a, 8.8, 8.11, inherited I3/I4 calibration | Defines the median of all 3N concurrent cold timings, single-process diagnostic, shared reference-cost boundary, common thread limits, exact host load thresholds and clean reruns, raw/normalized/solve-count reporting and per-resolution cost allocation. |
| 7.c2, F1, F3–F5, R18 half-current | Makes the full selection-eligibility conjunction and contamination exclusion explicit; retains all consideration/rejection reasons; specifies common wall-time earliest-crossing selection/ties; optional buildable diagnostics get no credit; final half-current means are compared with theta0. |
| R18 resources | Defines request/run sampling, user/system self-plus-reaped-child CPU once, incompleteness after interruption, independent child exit receipts, and scoped lifetime RSS/raw units/unavailable values. |
| 7.6c, 8.G1/G4/G5, operations | Adds reproducibility prerequisites: reviewed merged implementation, selected host, model/design-bit/source/mesh records, complete settings supplement and demonstrated recovery without silent budget/calibration changes. |

## Still open

This revision does **not** turn protocol descriptions into implementation or
experimental evidence. Five visible pending markers remain for:

1. Frozen L0 derivative-sweep points/directions/steps/results and reviewed
   mixed-resolution wiring evidence.
2. Exact parameter/group/box tables and a final supplement link for the
   existing geometry-bound derivation.
3. Complete smooth/discrete guard/scale inventory, implied physical floors,
   excluded designs and regression/derivative evidence.
4. Actual host, process count, environment, calibration and source/data hashes.
5. Final supplement/artifact links, independent implementation review,
   recovery evidence and selected host.

The full surrogate algorithm and all optimizer/refusal options still need a
versioned supplement. The paired study still needs main runs, trace review,
final refinement checks and measured results. The pilot's coupled-geometry
failure motivation should be connected to the existing Results narrative
when this fragment is integrated. Existing immutable manuscript versions
and the accepted protocol are unchanged.

## Verification

Rendered with `pdflatex -halt-on-error -interaction=nonstopmode` inside a
standalone 10-point article wrapper using one-inch margins, `amsmath`,
`amssymb`, `booktabs`, `hyperref` and a prospective-draft header. After the
final text adjustment the preview is four pages, with no overfull/underfull
boxes, undefined references or LaTeX warnings. Five pending markers remain.
The first and last preview pages were visually inspected. This is a fragment
layout check, not the final paper's compilation or publication audit.

Reproduce by placing `\input{main-study-methods-draft.tex}` in that article
wrapper, compiling from this refs directory into a separate output directory,
and rerunning to a reference fixpoint. The temporary inspection PDF was
`/tmp/codex-motor-methods-coverage-preview/main.pdf`; it is not publication
provenance or a permanent experimental artifact.
