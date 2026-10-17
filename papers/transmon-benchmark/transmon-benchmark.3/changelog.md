# Changelog: transmon-benchmark v2 → v3

Reframe revision (Branch A — measured). Reviser drafted main.tex; the bibliography
merge, changelog, and render gate were completed separately after a mid-run
interruption (all mechanical-tail steps; main.tex content is the reviser's).

## Major changes (source → change)

- **BRIEF thesis reframe** → title + intro now lead with the tensor-compiler
  viability thesis; expanded architecture section (batched element kernels,
  matrix-free apply, on-device Krylov, f32/mixed-precision constraints). The
  transmon cross-validation is the validation vehicle, not the paper's identity.
- **Framing decision gate = Branch A** (GPU scaling returned an honest negative)
  → performance framed as architecture trajectory, not demonstrated advantage.
- **GPU scaling data FINAL** (benchmarks/gpu_driven_scaling/results.toml) → new
  four-config × four-size same-host table + honest analysis (GPU-f32 not
  competitive ≤26k edges: launch overhead + f32 ceiling + no preconditioner;
  lone directional positive = GPU-vs-same-algo-CPU → parity at ~26k edges).
- **CPU-cell artifact committed** (benchmarks/transmon_bench_cpu/results.toml) →
  resolves the v1-audit provenance gap; 51.2 s benchmarked at commit 3174015,
  with the note that subsequently-merged reorthogonalization caching (PR #510)
  cut the same solve to ~21 s (stated, table not silently updated).
- **New related-work axis** (transmon-benchmark.2.litsearch, 23 verified entries
  merged into refs.bib) → form compilers (FFC/UFL/Firedrake/TSFC) vs libCEED
  (the domain-specific-JIT foil Palace runs on); ML-framework FEM (JAX-FEM,
  PyTorch-FEA); **concurrent-work citation wen2026learning (TensorGalerkin,
  arXiv:2602.05052)**; differentiable-EM cluster; Halide/TVM lineage. Wedge
  keeps its three qualifiers (H(curl) full-wave / general-purpose ML stack /
  cross-validated at production accuracy).
- **Tree-cotree citations** (manges1995generalized, albanese1988solution) close
  the v1-review dim-8 gap in the gauge/spurious-mode discussion.
- **Eigen-gauge saga upgraded** → honest-physics section moves from
  "disclosed + filtered" to "diagnosed, characterized, resolved": tree-cotree
  1.64% spectrum shift (measured), bulk div-free deflates the junction mode
  (div-ratio 50.2 measured), port-aware rank-1 re-admission lands six modes
  ≤0.029%; the spurious 3.45 GHz mode characterized as a port artifact
  (L-scaling 1/√2, 99.4% port-localized).
- **Quantum layer** → one forward-looking discussion paragraph (E_C from
  tensor-ε capacitance, ω01/α via Koch oracle, C_Σ model-definition finding);
  deliberately not expanded (separate paper's scope).
- **Authors resolved** → \author{Robb Walters \and Crutcher Dunnavant} (equal
  contribution footnote on both). Affiliations remain TODO(operator). Venue
  targets physics.comp-ph (cross-list quant-ph, cs.MS).

## Carried as TODO(operator)
- Author affiliations; arXiv endorsement (see BRIEF submission-logistics).
- The 23-page length exceeds the ~15-16pp target — flag for the v3 review pass
  to assess trimming (the reframe expanded scope legitimately; reviewer's call).
