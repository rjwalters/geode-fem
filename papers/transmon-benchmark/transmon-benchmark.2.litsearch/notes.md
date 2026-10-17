# Litsearch notes — transmon-benchmark (re-run at v2)

Run 2026-07-14 by pub-litsearch with `web_search: true` (per-thread BRIEF
frontmatter). This is the post-draft RE-RUN sibling for version 2, driven by
(a) the 2026-07-14 BRIEF reframe around tensor compilers for EM simulation
and (b) the v1 review's dim-8 tree–cotree citation gap. It EXTENDS
`transmon-benchmark.0.litsearch/` — nothing from that run (or from v2's
`refs.bib`) is duplicated; `candidates.bib` here contains only NEW,
resolver-verified entries (24). Every entry passed the
resolver-verified-or-dropped chokepoint (`anvil/lib/cite.py`: Crossref for
DOIs, arXiv API for arXiv IDs). Unverifiable hits are in "Web leads
(unverified)" below and MUST NOT be cited.

## Positioning summary

**The reframed thesis and its related-work burden.** The paper now claims
that general-purpose ML tensor-compiler stacks (PyTorch/JAX/Burn-class) are
a viable foundation for production-grade computational electromagnetics,
with the same-mesh Palace cross-validation as the evidence standard. That
claim owes the reader a related-work axis the v0 litsearch never hunted:
who else has put FEM — or EM simulation generally — on these stacks? The
answer, after systematic search (trail below), splits into five clusters,
and the paper's wedge survives with one mandatory qualification (Cluster C).

**Cluster A — form compilers and code-generation FEM (the domain-specific
ancestor).** FEM has generated code from high-level weak-form descriptions
for two decades: FFC (`kirby2006compiler`, TOMS 2006), the UFL language
(`alnæs2014unified`, TOMS 2014), Firedrake (`rathgeber2017firedrake`, TOMS
2016/17) and its TSFC form compiler (`homolya2018tsfc`, SISC 2018). The
modern GPU expression of this tradition is libCEED (`brown2021libceed`,
JOSS 2021) — matrix-free, high-order, element-based operator evaluation —
plus MFEM's partial-assembly GPU path (already cited as `andrej2024high` in
v2's refs.bib; no new entry needed). The positioning move the draft must
make: **Palace itself sits on this stack** — MFEM with libCEED backends —
i.e., the reference solver uses a *domain-specific* element-kernel JIT
built by and for the FEM community. GEODE-FEM's bet is categorically
different: outsource kernel generation, scheduling, and backend portability
to a *general-purpose ML tensor stack* (Burn/cubecl) whose engineering is
amortized across the entire ML ecosystem, and express assembly and
matrix-free apply as batched tensor algebra (`[n_elem,6,6]` batched
matmuls, gather/scatter-add). Form compilers generate FEM kernels; the
tensor-compiler approach *reuses someone else's compiler* and phrases FEM
as its native workload. Citing A prominently is what makes the thesis
falsifiable rather than rhetorical.

**Cluster B — the general-purpose tensor-compiler lineage.** The intro's
"why ML-stack infrastructure maps onto FEM" paragraph can anchor the
compiler tradition itself: Halide's algorithm/schedule decoupling
(`ragankelley2013halide`, PLDI 2013) and TVM's end-to-end tensor-program
optimization (`chen2018tvm`, OSDI 2018) are the intellectual line that
cubecl/burn-cuda inherit. Searched for peer-reviewed *EM or FEM* built
directly on Halide/TVM/Exo: none found (absence trail below) — the
scientific-computing uptake of this lineage has been via the ML frameworks,
which is itself a point in the paper's favor.

**Cluster C — FEM on general-purpose ML frameworks (the wedge cluster; one
genuine near-neighbor found).** JAX-FEM (`xue2023jax`, CPC 2023) is the
canonical citation: nodal-Lagrange continuum-mechanics FEM on JAX, GPU
via XLA, differentiable — no H(curl), no Maxwell, no eigenproblems.
PyTorch-FEA (`liang2023pytorch`, CMPB 2023) is the biomechanics analog.
**The load-bearing find of this run is TensorMesh/TensorGalerkin
(`wen2026learning`, arXiv:2602.05052, ETH CamLab, Feb 2026; companion
sparse-solver library torch-sla, `chi2026torch`, arXiv:2601.13994):** a
general, fast, differentiable Galerkin-assembly framework on PyTorch whose
demo suite *includes magnetostatics via a stabilized nodal curl–curl
formulation*. This is concurrent work the paper MUST cite — a referee will
find it. It does not break the wedge: TensorMesh is nodal (not
H(curl)-conforming Nédélec), its EM examples are magnetostatic (not
full-wave, not generalized eigenproblems with lumped-element loading), and
it reports no cross-validation against a reference EM solver at
production accuracy. But it proves the genus "FEM assembly as batched
tensor ops on an ML framework" now exists independently — which *supports*
the paper's architectural thesis while narrowing its novelty claim. The
wedge must therefore be phrased with all three qualifiers: to our
knowledge, no **full-wave 3D H(curl)/Nédélec** FEM solver has been built
on a general-purpose ML tensor stack **and validated at production
accuracy against the reference solver** (0.032% on identical meshes).
That statement survived this run's searches ("Nédélec PyTorch", "H(curl)
JAX", "Maxwell eigenvalue GPU tensor framework", "Rust Burn finite element
electromagnetics", and variants — trail below).

**Cluster D — differentiable / ML-framework EM (structured grids, not
FEM).** The differentiable-EM literature is FDTD/FDFD/RCWA on rectilinear
grids: ceviche's autograd FDFD/FDTD (`hughes2019forward`, ACS Photonics
2019), Google's JAX-based inverse design under foundry constraints
(`schubert2022inverse`), the Meep adjoint pipeline (`hammond2022high`,
Opt. Express 2022 — autograd/JAX wrapped around a classical C++ FDTD
core), the JAX-native FDTDX framework (`mahlau2024flexible` arXiv 2024;
JOSS version `mahlau2026fdtdx`), and Meent's differentiable RCWA
(`kim2024meent`). GPU-commercial FDTD (Tidy3D) has only a whitepaper
(lead). The draft's distinction: these codes get Maxwell onto
ML frameworks by accepting structured grids and low-order stencils; the
transmon problem *requires* body-fitted tets, anisotropic sapphire, and
H(curl) conformity — exactly the regime the differentiable-EM literature
has not reached. Adjacent but distinct: purpose-built differentiable
kernel DSLs (DiffTaichi `hu2019difftaichi`; PhiFlow `holl2020learning`)
and neural-ansatz Maxwell eigensolvers (FieldTNN `jiang2024fieldtnn`,
where the network *replaces* the discretization rather than executing it —
cite to preempt "ML for Maxwell eigenproblems exists" confusion).

**Cluster E — GPU FEM for EM, the hand-written-kernel tradition.** The
pre-tensor-compiler baseline: hand-crafted CUDA assembly/solvers for
electromagnetic FEM (`huantingmeng2014gpu`, IEEE AP Magazine 2014, the
standard survey-style reference), and the modern matrix-free high-order
GPU line already covered by `andrej2024high` + `brown2021libceed`. One
sentence in related work: the tensor-compiler approach targets the same
hardware without bespoke kernel engineering, trading peak specialization
for portability (CPU/CUDA/WebGPU from one codebase) — with the honest
caveat that today it covers assembly + driven solves, not sparse direct
factorization (issues #502/#503).

**Mission 2 — tree–cotree gauging (v1 review dim-8 fix).** Both canonical
anchors are now verified: `albanese1988solution` (Albanese & Rubinacci,
IEEE Trans. Magn. 24(1):98–101, 1988 — the origin-era eddy-current
formulation work that introduced tree/cotree edge decompositions for
uniqueness) and `manges1995generalized` (Manges & Cendes, IEEE Trans.
Magn. 31(3):1342–1347, 1995 — "A generalized tree-cotree gauge for
magnetic field computation", the citation the spurious-mode future-work
sentence at v2 main.tex §8 (L473–474) actually needs). The reviser should
cite `manges1995generalized` there, optionally alongside
`albanese1988solution` for the lineage.

## Absence-of-artifact search trail (wedge verification)

Documented so the "to our knowledge no..." claim is auditable. All
searches 2026-07-14:

- `"Nédélec" OR "H(curl)" finite element PyTorch OR JAX OR "tensor
  framework" Maxwell full-wave solver` → classical H(curl) literature +
  **TensorMesh** (nodal curl–curl magnetostatics on PyTorch — verified as
  `wen2026learning`, discussed in Cluster C). No H(curl)-conforming
  full-wave hit.
- `Maxwell eigenvalue solver GPU machine learning framework "edge
  elements"` → FieldTNN (neural ansatz, verified `jiang2024fieldtnn`);
  photonic-crystal GPU kernel-compensation solver (custom CUDA, not an ML
  stack). No FEM-on-tensor-framework eigensolver.
- `"finite element" Maxwell electromagnetics solver built on JAX GPU
  "H(curl)" OR "edge element" full-wave 3D` → Jaxwell (JAX FDFD, not FEM,
  GitHub-only → lead); JAX-FEM (mechanics, nodal). Nothing H(curl).
- `finite element solver Rust Burn OR candle OR tch machine learning
  tensor framework electromagnetics` → Rust-ML framework comparisons,
  Palace itself, Meent (RCWA, verified `kim2024meent`). **No third-party
  Rust/Burn-class FEM or EM solver found** — geode-fem appears to be the
  first.
- Halide/TVM/Exo peer-reviewed EM/FEM use: searched in the course of the
  Cluster B hunts; found compiler-domain follow-ons (Tiramisu, Fireiron,
  scheduling papers), no EM/FEM application paper.
- Tidy3D paper hunt → whitepaper PDF + docs only; the only arXiv artifacts
  are third-party comparisons (e.g., Lumerical-vs-Tidy3D, arXiv:2506.16665,
  not needed). No first-party citable paper → lead.
- NVIDIA Warp (Macklin) → GTC 2022 talk + GitHub CITATION bibtex, no
  DOI/arXiv → lead. flaport/fdtd (Laporte) → GitHub/PyPI/docs only, no
  paper, no Zenodo DOI surfaced → lead.

## Confirmed coverage

- Form-compiler / code-gen FEM axis: NOW COVERED (`kirby2006compiler`,
  `alnæs2014unified`, `rathgeber2017firedrake`, `homolya2018tsfc`,
  `brown2021libceed`; MFEM-GPU via existing `andrej2024high`).
- Tensor-compiler lineage: covered (`ragankelley2013halide`, `chen2018tvm`).
- FEM-on-ML-frameworks: covered (`xue2023jax`, `wen2026learning`,
  `chi2026torch`, `liang2023pytorch`).
- Differentiable EM: covered (`hughes2019forward`, `schubert2022inverse`,
  `hammond2022high`, `mahlau2024flexible`/`mahlau2026fdtdx`, `kim2024meent`)
  plus DSL/neural-ansatz boundary markers (`hu2019difftaichi`,
  `holl2020learning`, `jiang2024fieldtnn`).
- GPU-EM FEM baseline: covered (`huantingmeng2014gpu` + existing entries).
- Tree–cotree gauging: CLOSED (`albanese1988solution`,
  `manges1995generalized`).
- Everything from the v0 run (systems under comparison, qubit workflows,
  V&V methodology, open ecosystems, numerics) remains valid and merged in
  v2's refs.bib.

## Identified gaps

The role names gaps; it does not invent placeholder entries.

- **Firedrake/Halide/UFL truncated Crossref titles** — cosmetic, flagged in
  candidates.bib header; fix at merge time (also rename the non-ASCII key
  `alnæs2014unified` → `alnaes2014unified`).
- **NVIDIA Warp** has no citable paper (lead); if the draft wants the
  "Python JIT for simulation kernels" example it must be a software/URL
  citation, author-supplied at revise time.
- **NVIDIA Modulus** was not hunted: it is a physics-ML (neural surrogate)
  framework, out of the FEM-on-tensor-stack genus; add only if the reviser
  wants a physics-ML boundary sentence beyond FieldTNN.
- Carried from v0, still open (author-supplied DOIs would promote): citable
  TEAM anchor; Bossavit's *Computational Electromagnetism* (1998) textbook;
  Ericsson–Ruhe spectral transformation; IEEE version of
  `ye2025electromagnetic`; burn-cuda/cubecl f64 GitHub-issue URL citation
  (main.tex L621 TODO(operator)).

## Re-run delta

The v0 sibling (28 candidates) targeted the original cross-validation
framing; its clusters and absence trails stand. This re-run responds to
two inputs: (1) the operator's 2026-07-14 BRIEF reframe, which makes
"EM/FEM on ML tensor-compiler stacks" the paper's central related-work
question — populated here as Clusters A–E with 22 new verified entries;
and (2) the v1 review's dim-8 `related-work` comment (comments.md item 1:
the uncited tree–cotree future-work sentence), closed with two verified
IEEE anchors. Net wedge assessment after searching: **the reframed claim
survives**, with one mandatory concurrent-work citation
(`wen2026learning`) and the three-qualifier phrasing given in Cluster C.

## Web provenance

| bib key | identifier | resolver |
|---|---|---|
| `kirby2006compiler` | doi:10.1145/1163641.1163644 | crossref |
| `alnæs2014unified` | doi:10.1145/2566630 | crossref |
| `rathgeber2017firedrake` | doi:10.1145/2998441 | crossref |
| `homolya2018tsfc` | doi:10.1137/17M1130642 | crossref |
| `brown2021libceed` | doi:10.21105/joss.02945 | crossref |
| `ragankelley2013halide` | doi:10.1145/2491956.2462176 | crossref |
| `chen2018tvm` | arXiv:1802.04799 | arxiv |
| `xue2023jax` | doi:10.1016/j.cpc.2023.108802 | crossref |
| `wen2026learning` | arXiv:2602.05052 | arxiv |
| `chi2026torch` | arXiv:2601.13994 | arxiv |
| `liang2023pytorch` | doi:10.1016/j.cmpb.2023.107616 | crossref |
| `hughes2019forward` | doi:10.1021/acsphotonics.9b01238 | crossref |
| `schubert2022inverse` | arXiv:2201.12965 | arxiv |
| `hammond2022high` | doi:10.1364/oe.442074 | crossref |
| `mahlau2024flexible` | arXiv:2412.12360 | arxiv |
| `mahlau2026fdtdx` | doi:10.21105/joss.08912 | crossref |
| `kim2024meent` | arXiv:2406.12904 | arxiv |
| `hu2019difftaichi` | arXiv:1910.00935 | arxiv |
| `holl2020learning` | arXiv:2001.07457 | arxiv |
| `jiang2024fieldtnn` | arXiv:2411.15828 | arxiv |
| `huantingmeng2014gpu` | doi:10.1109/map.2014.6837065 | crossref |
| `albanese1988solution` | doi:10.1109/20.43865 | crossref |
| `manges1995generalized` | doi:10.1109/20.376275 | crossref |

(One search-reported DOI was wrong and was corrected before verification:
the Hammond et al. Meep-adjoint paper surfaced as 10.1364/OE.448426, which
404s at Crossref; a Crossref title query returned the true DOI
10.1364/oe.442074, which resolved — exact title/author match.)

## Web leads (unverified)

Leads carry no BibTeX key and MUST NOT be cited by the drafter or reviser.
Promote a lead by supplying a resolvable DOI / arXiv ID and re-running
litsearch.

| title | authors | year | URL as found | reason unresolved |
|---|---|---|---|---|
| Warp: A High-performance Python Framework for GPU Simulation and Graphics | Macklin, Miles | 2022 | https://github.com/nvidia/warp | no identifier extractable (GTC talk + GitHub CITATION bibtex; no DOI or arXiv ID exists) |
| Tidy3D: hardware-accelerated electromagnetic solver for fast simulations at scale | Flexcompute Inc. | 2022 | https://www.flexcompute.com/assets/tidy3d/tidy3d__hardware_accelerated_electromagnetic_solver_for_fast_simulations_at_scale.pdf | no identifier extractable (vendor whitepaper; no first-party paper found) |
| Jaxwell: GPU-accelerated, differentiable 3D iterative FDFD electromagnetic solver | Fischbach, Jan-David (maintainer fork; orig. Stanford) | n.d. | https://jan-david-fischbach.github.io/jaxwell/ | no identifier extractable (GitHub/docs-only software; FDFD not FEM) |
| fdtd: A 3D electromagnetic FDTD simulator written in Python (PyTorch backend) | Laporte, Floris | 2020 | https://github.com/flaport/fdtd | no identifier extractable (GitHub/PyPI only; no paper or Zenodo DOI found) |
| TensorMesh: a fast, differentiable, JIT-free finite element library for PyTorch (software) | CamLab, ETH Zurich | 2026 | https://github.com/camlab-ethz/TensorMesh | no identifier for the software itself; the companion PAPER is verified as `wen2026learning` — cite that, plus a software/URL note if the repo is named |
