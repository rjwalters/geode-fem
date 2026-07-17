# Litsearch notes — transmon-benchmark.4.litsearch (re-run for the v5 differentiable-design reframe)

Re-run of `pub-litsearch` after the v4 review, driven by the ⭐ 2026-07-16 reframe
in `BRIEF.md` (item 5, CITATION-HYGIENE WARNING) and the reviewer's §B
`related-work` leads in `transmon-benchmark.4.review/comments.md`. `web_search:
true` is set in the BRIEF frontmatter; every web-discovered entry below passed
the resolver-verified-or-dropped contract via `anvil/lib/cite.py::resolve()`.

## Positioning summary

**Cluster 1 — superconducting-qubit design optimization (the motivation
cluster).** The reframed intro's core motivation — qubit design today is an
iterative loop around non-differentiable EM simulation — is anchored by TWO
papers, and the brief's original mapping collapsed them incorrectly.
The important discovery of this re-run: **arXiv:2508.18027 IS the
QDesignOptimizer paper** (`eriksson2025automated`, Eriksson, Splitthoff, et
al., Chalmers 202Q-lab, 2025 — "we provide a full implementation of our
optimization method as an open-source Python package, QDesignOptimizer").
The brief treated "guess-and-check unmet need (2508.18027)" and
"QDesignOptimizer (find the ID)" as two citations; they are ONE. Its abstract
motivates via "time-consuming iterative electromagnetic simulations requiring
manual intervention" and closes the loop with "user-defined, physics-informed,
nonlinear models that guide parameter updates" (HFSS + pyEPR + Qiskit-Metal
stack). ⚠️ Wording caution for the reviser: the abstract does NOT literally
say "HFSS is not differentiable" — phrase the claim as: the state-of-practice
workflow iterates non-differentiable EM simulations and injects
analytic/physics-model updates precisely because solver-derived gradients are
unavailable, citing `eriksson2025automated` for the workflow itself. GEODE's
contribution (solver-level discrete-adjoint gradients from the same FEM solve
that produces the observables) is exactly what that loop lacks.

**Cluster 2 — lumped-circuit-level differentiation (SQcircuit).** The correct
identifiers, both resolver-verified: the original package paper is
`rajabzadeh2023analysis` (Quantum 7, 1118, 2023; DOI
10.22331/q-2023-09-25-1118; the arXiv version is 2206.08319), and the
gradient-optimization follow-up is `rajabzadeh2024general` (arXiv:2408.12704,
Rajabzadeh, Boulton-McKeehan, Bonkowsky, Schuster, Safavi-Naeini, 2024) —
the paper the brief had mislabeled as QDesignOptimizer. ⚠️ Claim-precision
caution: 2408.12704's abstract names "computing the gradients of eigenvalues
and eigenvectors of a Hamiltonian — a large, sparse matrix — ... a significant
challenge" and then ADDRESSES it (PyTorch autodiff integrated into SQcircuit).
Do NOT write that SQcircuit "notes sparse-eigenpair gradients as an open gap"
— it solves that problem at the lumped-circuit level. The honest wedge: the
Rajabzadeh line differentiates the *circuit Hamiltonian* built from lumped
parameters; the map from *geometry/materials to those parameters* still runs
through non-differentiable 3D EM simulation — that distributed/FEM-level
gradient is what this paper supplies.

**Cluster 3 — differentiable EM owned by photonics.** Confirmed and covered:
FDTDX is already cited as `mahlau2026fdtdx` (JOSS 2026, DOI
10.21105/joss.08912) — sufficient on its own; the arXiv methods paper
`mahlau2024flexible` (arXiv:2412.12360, note the different title "A flexible
framework for large-scale FDTD simulations...") is supplied as an optional
companion if the reviser wants the methods rather than software citation.
TorchGDM confirmed as `ponomareva2025torchgdm` (arXiv:2505.09545, Ponomareva,
..., Wiecha; PyTorch GDM integral-method scattering with autodiff; published
in SciPost Physics Codebases). Added `molesky2018inverse` (Nature Photonics
2018) as the canonical "photonics owns inverse design/adjoint EM" anchor —
the existing refs (hughes2019forward, hammond2022high, schubert2022inverse,
kim2024meent) are all instances; Molesky et al. is the citable genus. The
wedge sentence stands: differentiable EM lives in photonics FDTD/integral
methods; frequency-domain FEM for RF/superconducting design is uncontested.

**Cluster 4 — differentiable FEM outside EM.** JAX-FEM venue verified: the
brief's "Nature Comp. Sci. 2023" is WRONG — the archival venue is **Computer
Physics Communications 291, 108802 (2023)**, DOI 10.1016/j.cpc.2023.108802,
which is exactly what the existing `xue2023jax` entry carries (Crossref
resolution this run returned identical fields; the resolver minted collision
key `xue2023jaxb`, confirming the match). No refs.bib change needed — the
reviser must simply not "fix" the venue toward the brief's guess.

**Cluster 5 — the eigenmode-differentiation roadmap.** VERDICT on
arXiv:2603.29718: **CONFIRMED — it supports the roadmap claim.** It is
"Adaptive Multilevel Methods for the Maxwell Eigenvalue Problem" (Liang, Xu,
Zhang; `liang2026adaptive`), and its abstract explicitly proposes "an adaptive
multilevel preconditioned Helmholtz-Jacobi-Davidson (PHJD) method for the
Maxwell eigenvalue problem with singularities", i.e., a preconditioned
Jacobi-Davidson iteration with Helmholtz projection — exactly the JD +
Helmholtz-projection path the spine names for escaping the σ=4.5 GHz
SPD-proxy preconditioner wall. Safe to anchor the roadmap paragraph on it.
For the "Hellmann-Feynman / adjoint-eigenpair formulas" half of the roadmap
sentence, added the canonical citable: `nelson1976simplified` (Nelson, AIAA
J. 14(9):1201-1205, 1976, DOI 10.2514/3.7211) — the standard single-eigenpair
eigenvector-derivative method requiring only the eigenpair itself, the
classical ancestor of adjoint-eigenpair differentiation.

## Confirmed coverage (already adequate in the v4 refs.bib)

- FDTDX: `mahlau2026fdtdx` (JOSS) — verified live; sufficient alone.
- JAX-FEM: `xue2023jax` (CPC 2023) — Crossref-verified this run; fields correct.
- SQuADDS: `shanto2024squadds` — the paper arXiv:2312.13483 actually is.
- Photonics differentiable-EM instances: `hughes2019forward` (Ceviche-line
  forward-mode Maxwell), `hammond2022high` (Meep adjoint topology
  optimization), `schubert2022inverse` (SPINS-class foundry-constrained
  inverse design), `kim2024meent` (RCWA for ML).
- ML-framework FEM/PDE adjacents: `liang2023pytorch` (PyTorch-FEA),
  `chi2026torch` (torch-sla adjoint sparse solvers), `wen2026learning`
  (TensorGalerkin, concurrent work), `hu2019difftaichi`, `holl2020learning`.
- Physics framing: `koch2007`, `minev2021` (EPR), `nigg2012` (BBQ),
  `minev2021circuit`.
- Palace/MFEM/DeviceLayout, form compilers, libCEED: unchanged from the
  .2.litsearch positioning.

## Identified gaps (leads only — no invented entries)

- **Palace/HFSS "structurally cannot produce solver-derived design
  gradients"**: per the brief, this rests on documented architecture, not a
  single citation. No citation exists to find; the reviser should cite the
  Palace docs/repo (`palace`) plus its libCEED substrate (`brown2021libceed`)
  and keep the claim architectural, not bibliographic.
- **QDesignOptimizer as software** (as distinct from the paper): the GitHub
  repo is url-kind, unresolvable in v0 (see Web leads). If a software
  citation is wanted alongside `eriksson2025automated`, the author can add a
  hand-written `@misc` with the repo URL, matching the existing
  `devicelayout`/`palace` pattern (author-supplied, outside the resolver
  contract).
- **Adjoint shape/mesh-morphing for the pad-scaling honest negative**: if the
  reviser wants a citation for mesh-morphing/shape-parametrization limits
  (the 33×-short θ cap), none was searched this run — the finding stands on
  its own data; search "CAD-consistent shape derivatives / mesh morphing in
  adjoint EM optimization, 2018-2025" if an anchor is desired.

## Re-run delta

The .2.litsearch (2026-07-14) served the tensor-compiler framing: form
compilers, libCEED, GPU-EM, TensorGalerkin concurrency. This re-run serves the
2026-07-16 differentiable-design reframe and the v4 reviewer's §B leads: it
corrects the brief's three misaligned IDs (2508.18027 = QDesignOptimizer
itself, merging two planned citations into one; 2408.12704 = the SQcircuit
gradient follow-up, not QDesignOptimizer; SQcircuit original = Quantum 2023 /
arXiv 2206.08319, since 2312.13483 is SQuADDS, already cited), confirms
TorchGDM (2505.09545) and the PHJD Maxwell-eigenvalue anchor (2603.29718),
verifies JAX-FEM's true venue (CPC 2023, not Nature Comp. Sci.), and adds two
canonical anchors the reframed intro needs (`molesky2018inverse`,
`nelson1976simplified`). All eight candidates.bib entries are web-discovered
and resolver-verified; nothing was carried on recall.

## Web provenance

| bib key | identifier | resolver |
|---|---|---|
| eriksson2025automated | arXiv:2508.18027 | arxiv |
| rajabzadeh2024general | arXiv:2408.12704 | arxiv |
| rajabzadeh2023analysis | doi:10.22331/q-2023-09-25-1118 | crossref |
| mahlau2024flexible | arXiv:2412.12360 | arxiv |
| ponomareva2025torchgdm | arXiv:2505.09545 | arxiv |
| liang2026adaptive | arXiv:2603.29718 | arxiv |
| molesky2018inverse | doi:10.1038/s41566-018-0246-9 | crossref |
| nelson1976simplified | doi:10.2514/3.7211 | crossref |

Verification-only resolutions (already in v4 refs.bib; NOT added to
candidates.bib): `xue2023jax` re-verified via doi:10.1016/j.cpc.2023.108802
(crossref; resolver returned identical fields under collision key
`xue2023jaxb`, discarded).

## Web leads (unverified)

| title | authors | year | URL as found | reason unresolved |
|---|---|---|---|---|
| QDesignOptimizer (software repository) | 202Q-lab (Chalmers) | 2025 | https://github.com/202Q-lab/QDesignOptimizer | url-kind unsupported in v0 (UnsupportedIdentifierError) |
| Qubit-Discovery (software repository) | stanfordLINQS | 2024 | https://github.com/stanfordLINQS/Qubit-Discovery | url-kind unsupported in v0 (UnsupportedIdentifierError) |
