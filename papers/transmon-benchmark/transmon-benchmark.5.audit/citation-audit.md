# Citation audit — transmon-benchmark.5

Audited: 2026-07-16. Auditor: pub-audit (Fable 5).

## Resolution check

- Enumerated every natbib cite command in `main.tex` (single-file paper; no
  `\input`/`\include` children — verified by grep). Multi-line `\citep{...}`
  blocks were flattened before extraction (a line-based scan undercounts by 4).
- **57 unique keys used across 65 cite commands; 57/57 resolve** to entries in
  `refs.bib`. **Zero unresolved. Zero unused bib entries.** The compiled
  `main.bbl` carries exactly 57 `\bibitem`s and the final PDF contains no
  `[??]` or unresolved `\ref`.
- This independently confirms the `.5.review` count (57/57).

## Claim-support spot-check

No `<thread>/refs/` directory exists, so most sources are not on disk and are
marked `unverified` per contract (not flagged). The seven keys newly merged at
v5 (from `transmon-benchmark.4.litsearch/notes.md`, resolver-verified there)
were **re-verified live this audit** against the arXiv export API /
Crossref — the verdicts below for those keys are first-hand, not carried from
the litsearch sibling.

### Verified this audit (live source metadata/abstracts)

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| eriksson2025automated | yes | §1 L112–117: QDesignOptimizer "iterates ``time-consuming electromagnetic simulations'' of HFSS-class solvers and guides the parameter updates with *separate* ... analytic physics models"; also abstract L68–73 and Related Work L243–247 | **partial — MISQUOTE (critical flag)** | Live arXiv:2508.18027 abstract reads "time-consuming **iterative** electromagnetic simulations requiring manual intervention". The paper's quotation marks claim a verbatim span the source does not contain (the word "iterative" is dropped). The *substance* of all three cite sites IS supported (abstract confirms: HFSS + pyEPR + Qiskit-Metal stack; "user-defined, physics-informed, nonlinear models that guide parameter updates"); the litsearch claim-precision caution (do not attribute "HFSS is not differentiable" to the abstract) is honored — the non-differentiability claim is framed architecturally, not bibliographically. Fix is one word (restore "iterative") or unquote to paraphrase. See flags.md. |
| rajabzadeh2023analysis | yes | §1 L127: "SQcircuit ... analyzes arbitrary superconducting circuits"; §2 L248 | supports | Live arXiv 2206.08319 title: "Analysis of arbitrary superconducting quantum circuits accompanied by a Python package: SQcircuit" (Quantum 7, 1118, 2023 per refs.bib). Exact match. |
| rajabzadeh2024general | yes | §1 L128–131: "gradient-based follow-up computes eigenvalue and eigenvector gradients of the circuit Hamiltonian via PyTorch autodiff, optimizing qubit designs in circuit-parameter space"; §2 L248–251 | supports | Live arXiv:2408.12704 abstract confirms verbatim substance ("computing the gradients of eigenvalues and eigenvectors of a Hamiltonian--a large, sparse matrix--... integrating automatic differentiation within SQcircuit"). The litsearch caution is honored: the paper credits it with *solving* lumped-level eigenpair gradients, and the honest wedge (geometry→parameters map remains simulation-bound) matches. |
| ponomareva2025torchgdm | yes | §2 L267–268: "GPU-accelerated autodiff integral-method scattering in TorchGDM"; §1 L137 (photonics cluster) | supports | Live arXiv:2505.09545 title/abstract: PyTorch GDM, GPU-enabled autodiff, derivatives of any observable. Exact match. |
| liang2026adaptive | yes | §6 L966–970: PHJD "adaptive-multilevel treatment of exactly this singular Maxwell eigenproblem" as the named roadmap (cited as method, not result) | supports | Live arXiv:2603.29718 title/abstract: "adaptive multilevel preconditioned Helmholtz-Jacobi-Davidson (PHJD) method for the Maxwell eigenvalue problem with singularities". Exact match to the roadmap sentence. |
| molesky2018inverse | yes | §1 L136, §2 L262: photonics owns adjoint/autodiff inverse-design EM (genus anchor) | supports | Crossref 10.1038/s41566-018-0246-9: "Inverse design in nanophotonics", Nature Photonics. Canonical genus citation; instances (hughes2019forward etc.) cited alongside. |
| nelson1976simplified | yes | §6 L947–949: "the adjoint-eigenpair (Hellmann--Feynman / Nelson) formulas" | supports | Crossref 10.2514/3.7211: "Simplified calculation of eigenvector derivatives", AIAA Journal, 1976. The classical single-eigenpair eigenvector-derivative method; matches the use. |
| xue2023jax | yes | §1 L138–139, §2 L273–274: differentiable FEM proven outside EM (JAX-FEM, solid mechanics) | supports | Venue integrity re-confirmed: refs.bib carries Computer Physics Communications 2023 (NOT the BRIEF's wrong "Nature Comp. Sci." guess); a refs.bib comment pins this. Litsearch re-resolved identical fields via Crossref. |

### Not verifiable on disk (recorded, not flagged — 49 keys)

Source PDFs are not in `<thread>/refs/` and were not re-fetched; per contract
these are `unverified — source not on disk`. The author is responsible for
off-disk verification. Grouped by role:

| Keys | Role | Verdict |
|---|---|---|
| palace, palace-blog, mfem, andrej2024high, devicelayout, devicelayout-blog, devicelayout-juliacon | Palace/MFEM/DeviceLayout stack (software + docs cites) | unverified — source not on disk |
| koch2007, nigg2012, minev2021, minev2021circuit, shanto2024squadds | transmon/EPR/BBQ physics framing | unverified — source not on disk |
| hughes2019forward, hammond2022high, schubert2022inverse, kim2024meent, mahlau2026fdtdx | photonics differentiable-EM instances | unverified — source not on disk |
| liang2023pytorch, chi2026torch, wen2026learning, hu2019difftaichi, holl2020learning | differentiable-FEM / ML-framework PDE adjacents | unverified — source not on disk |
| burn, kazdadi2026faer, gmsh, arpack, slepc, arsenovic2022scikit | software substrate cites | unverified — source not on disk |
| alnaes2014unified, logg2010dolfin, kirby2006compiler, homolya2018tsfc, rathgeber2017firedrake, ragankelley2013halide, chen2018tvm, brown2021libceed | form-compiler / tensor-compiler lineage | unverified — source not on disk |
| nedelec1980, hiptmair-xu, tzaniovkolevpanayotsvassilevski2018parallel, dular1998general, albanese1988solution, manges1995generalized | FEM/gauge/preconditioning classics | unverified — source not on disk |
| huantingmeng2014gpu, jiang2024fieldtnn, ye2025electromagnetic, sommers2025open, hoffmann2009comparison, oberkampf2010verification, roache1997quantification | GPU-EM / V&V / benchmarking context | unverified — source not on disk |

### Litsearch claim-precision cautions (all four checked)

1. **QDesignOptimizer = arXiv:2508.18027 (one paper, one key)** — honored;
   `eriksson2025automated` serves both the unmet-need motivation and the
   package citation. No phantom second key.
2. **SQcircuit line credited with solving lumped-level eigenpair gradients**
   (not "notes it as an open gap") — honored at both cite sites.
3. **"HFSS is not differentiable" not attributed to the eriksson abstract** —
   honored; §1 L117–123 frames non-differentiability architecturally
   (hand-written kernels, no adjoint exposed), cites only `palace` for the
   open-source instance.
4. **xue2023jax stays CPC 2023** — honored (refs.bib comment pins it).

## Uncited-claims scan

No critical gaps found. One note: the §1 architectural claim that commercial
solvers (HFSS, COMSOL) have "no derivative path" and "none exposes an adjoint
of its solve" (L120–123) is uncited and — per the litsearch gap analysis — has
no citable source; for closed-source products it is unverifiable by
construction. It is appropriately framed as architecture rather than
bibliography, and the wedge claim carries a "to our knowledge" hedge (L144).
Recorded as a non-critical note for author awareness; no flag.
